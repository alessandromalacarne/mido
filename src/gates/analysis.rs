//! Gate 3 — maintainability metrics.

use crate::config::{value, Config};
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::metrics::{Measurements, Unit};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;

pub fn analysis_tool(config: &Config, target: &Target, lang: Lang) -> String {
    config.text_setting("analysis", "tool", lang.metrics_tool(), Some(&target.name))
}

pub fn gate_analysis(run: &GateRun<'_>, measurements: &Measurements) -> GateResult {
    let GateRun {
        target,
        config,
        lang,
        ..
    } = *run;
    let mi_min = mi_floor(config, target);
    let cognitive_max = config.float("analysis", "cognitive_max", 15.0, Some(&target.name));
    let tool = analysis_tool(config, target, lang);
    let contract = analysis_contract(config, &tool, mi_min, cognitive_max);

    if let Some(result) = unsupported_tool(&tool, &contract, lang) {
        return result;
    }

    if let Some(result) = unreadable(&measurements.errors, &contract) {
        return result;
    }

    let Some(worst_cognitive) = measurements.units.iter().max_by_key(|unit| unit.cognitive) else {
        return no_metrics(contract);
    };

    let missing = measurements
        .units
        .iter()
        .filter(|unit| unit.mi.is_none())
        .count();
    if mi_min.is_some() && missing > 0 {
        return missing_index(missing, measurements.units.len(), &tool, contract);
    }

    let worst_mi = worst_index(&measurements.units);
    let problems = judge_units(&measurements.units, mi_min, cognitive_max);
    let status = if problems.is_empty() { PASS } else { FAIL };
    let mut details = vec![
        index_line(worst_mi, mi_min, &tool),
        cognitive_line(worst_cognitive, cognitive_max),
    ];
    details.extend(problems);

    GateResult::new(
        "analysis",
        status,
        summary_line(worst_mi, worst_cognitive),
        details,
    )
    .contract(contract)
    .fixes(Gate::Analysis.fix_hints().iter().copied())
}

/// The maintainability index floor this target wrote down; `None` when nobody
/// set one, and then no floor is enforced.
fn mi_floor(config: &Config, target: &Target) -> Option<f64> {
    config
        .section("analysis", Some(&target.name))
        .get("mi_min")
        .and_then(value::as_float)
}

/// One file the tool could not read makes every reading incomplete: the gate
/// judges what it measured, and it did not measure everything.
fn unreadable(errors: &[String], contract: &str) -> Option<GateResult> {
    let reason = errors.first()?;
    Some(
        GateResult::new("analysis", INCOMPLETE, reason.clone(), errors.to_vec())
            .contract(contract.to_string())
            .fixes(Gate::Analysis.fix_hints().iter().copied()),
    )
}

/// No readings at all: INCOMPLETE — a gate with nothing to judge is never a
/// pass.
fn no_metrics(contract: String) -> GateResult {
    GateResult::new(
        "analysis",
        INCOMPLETE,
        "no function metrics to judge",
        Vec::<String>::new(),
    )
    .contract(contract)
    .fixes(Gate::Analysis.fix_hints().iter().copied())
}

/// A floor the stack cannot back with readings is a gate that cannot run: say
/// so, instead of quietly judging something else.
fn missing_index(missing: usize, total: usize, tool: &str, contract: String) -> GateResult {
    let summary = if missing == total {
        format!("no maintainability index ({tool} reports none)")
    } else {
        format!("{missing} of {total} functions have no maintainability index")
    };
    GateResult::new(
        "analysis",
        INCOMPLETE,
        summary,
        [format!(
            "[analysis] mi_min is set, and {tool} reports no maintainability index here"
        )],
    )
    .contract(contract)
    .fixes([
        "drop [analysis] mi_min for this target, or measure it with a tool that reports a maintainability index",
    ])
}

/// The contract line the gate cites, whatever its verdict.
fn analysis_contract(
    config: &Config,
    tool: &str,
    mi_min: Option<f64>,
    cognitive_max: f64,
) -> String {
    match mi_min {
        Some(mi_min) => format!(
            "{} [analysis] mi_min={mi_min}, cognitive_max={cognitive_max} (tool {tool})",
            config.source()
        ),
        None => format!(
            "{} [analysis] cognitive_max={cognitive_max} (tool {tool}, which reports no maintainability index)",
            config.source()
        ),
    }
}

/// A `tool` this module cannot drive is INCOMPLETE, never a pass.
fn unsupported_tool(tool: &str, contract: &str, lang: Lang) -> Option<GateResult> {
    let supported = lang.metrics_tool();
    if tool == supported {
        return None;
    }

    Some(
        GateResult::new(
            "analysis",
            INCOMPLETE,
            format!("tool `{tool}` is not supported by this module"),
            [format!(
                "`.mido.toml` names `{tool}`; the {} module measures with {supported}",
                lang.name()
            )],
        )
        .contract(contract)
        .fixes([format!(
            "point [analysis] tool at {supported}, or extend the module with a parser for that tool"
        )]),
    )
}

/// The lowest maintainability index, and the unit it belongs to.
fn worst_index(units: &[Unit]) -> Option<(&Unit, f64)> {
    units
        .iter()
        .filter_map(|unit| unit.mi.map(|mi| (unit, mi)))
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
}

fn summary_line(worst_mi: Option<(&Unit, f64)>, worst_cognitive: &Unit) -> String {
    match worst_mi {
        Some((_, mi)) => format!("worst MI {mi:.1}, cognitive {}", worst_cognitive.cognitive),
        None => format!(
            "cognitive {} (no MI in this stack)",
            worst_cognitive.cognitive
        ),
    }
}

fn index_line(worst_mi: Option<(&Unit, f64)>, mi_min: Option<f64>, tool: &str) -> String {
    match worst_mi {
        Some((unit, mi)) => {
            let floor = mi_min
                .map(|min| format!(" (min {min})"))
                .unwrap_or_default();
            format!("worst MI: {mi:.1}{floor} — {} @ {}", unit.name, unit.path)
        }
        None => format!("MI: not measured ({tool} reports no maintainability index)"),
    }
}

fn cognitive_line(unit: &Unit, cognitive_max: f64) -> String {
    format!(
        "worst cognitive: {} (max {}) — {} @ {}",
        unit.cognitive, cognitive_max, unit.name, unit.path
    )
}

fn judge_units(units: &[Unit], mi_min: Option<f64>, cognitive_max: f64) -> Vec<String> {
    let mut problems = Vec::new();
    for unit in units {
        if let (Some(mi), Some(min)) = (unit.mi, mi_min) {
            if mi < min {
                problems.push(format!(
                    "{}: {} MI {:.1} (min {})",
                    unit.path, unit.name, mi, min
                ));
            }
        }
        if unit.cognitive as f64 > cognitive_max {
            problems.push(format!(
                "{}: {} cognitive complexity {} (max {})",
                unit.path, unit.name, unit.cognitive, cognitive_max
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests;
