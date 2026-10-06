//! Gate 3 — maintainability metrics.

use crate::config::Config;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::metrics::Unit;
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;

pub fn analysis_tool(config: &Config, target: &Target, lang: Lang) -> String {
    config.text_setting("analysis", "tool", lang.metrics_tool(), Some(&target.name))
}

pub fn gate_analysis(run: &GateRun<'_>, units: &[Unit], tool_errors: &[String]) -> GateResult {
    let GateRun {
        target,
        config,
        lang,
        ..
    } = *run;
    let (mi_min, cognitive_max) = analysis_limits(config, target);
    let tool = analysis_tool(config, target, lang);
    let contract = analysis_contract(config, &tool, mi_min, cognitive_max);

    if let Some(result) = unsupported_tool(&tool, &contract, lang) {
        return result;
    }

    let Some((worst_mi, worst_cognitive)) = worst_units(units) else {
        return no_metrics(tool_errors, contract);
    };

    let problems = judge_units(units, mi_min, cognitive_max);
    let mut details = vec![
        worst_mi_line(worst_mi, mi_min),
        worst_cognitive_line(worst_cognitive, cognitive_max),
    ];
    let status = if problems.is_empty() { PASS } else { FAIL };
    details.extend(problems);

    GateResult::new(
        "analysis",
        status,
        format!(
            "worst MI {:.1}, cognitive {}",
            worst_mi.mi, worst_cognitive.cognitive
        ),
        details,
    )
    .contract(contract)
    .fixes(Gate::Analysis.fix_hints().iter().copied())
}

/// No metrics to judge: INCOMPLETE, with the first tool error as the reason.
fn no_metrics(tool_errors: &[String], contract: String) -> GateResult {
    let detail = tool_errors
        .first()
        .cloned()
        .unwrap_or_else(|| "no function metrics to judge".to_string());
    GateResult::new("analysis", INCOMPLETE, detail, tool_errors.to_vec())
        .contract(contract)
        .fixes(Gate::Analysis.fix_hints().iter().copied())
}

/// The `[analysis]` floors for one target.
fn analysis_limits(config: &Config, target: &Target) -> (f64, f64) {
    (
        config.float("analysis", "mi_min", 20.0, Some(&target.name)),
        config.float("analysis", "cognitive_max", 15.0, Some(&target.name)),
    )
}

/// The contract line the gate cites, whatever its verdict.
fn analysis_contract(config: &Config, tool: &str, mi_min: f64, cognitive_max: f64) -> String {
    format!(
        "{} [analysis] mi_min={mi_min}, cognitive_max={cognitive_max} (tool {tool})",
        config.source()
    )
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
            format!("tool `{tool}` is not supported by this runner"),
            [format!(
                "`.mido.toml` names `{tool}`; this module only drives {supported}-cli"
            )],
        )
        .contract(contract)
        .fixes([format!(
            "point [analysis] tool at {supported}, or extend the module with a parser for that tool"
        )]),
    )
}

fn worst_units(units: &[Unit]) -> Option<(&Unit, &Unit)> {
    let worst_mi = units
        .iter()
        .min_by(|left, right| left.mi.total_cmp(&right.mi))?;
    let worst_cognitive = units.iter().max_by_key(|unit| unit.cognitive)?;
    Some((worst_mi, worst_cognitive))
}

fn worst_mi_line(unit: &Unit, mi_min: f64) -> String {
    format!(
        "worst MI: {:.1} (min {}) — {} @ {}",
        unit.mi, mi_min, unit.name, unit.path
    )
}

fn worst_cognitive_line(unit: &Unit, cognitive_max: f64) -> String {
    format!(
        "worst cognitive: {} (max {}) — {} @ {}",
        unit.cognitive, cognitive_max, unit.name, unit.path
    )
}

fn judge_units(units: &[Unit], mi_min: f64, cognitive_max: f64) -> Vec<String> {
    let mut problems = Vec::new();
    for unit in units {
        if unit.mi < mi_min {
            problems.push(format!(
                "{}: {} MI {:.1} (min {})",
                unit.path, unit.name, unit.mi, mi_min
            ));
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
