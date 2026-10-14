//! Gate 2 — size and complexity ceilings.

use crate::config::{Config, Threshold};
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::metrics::{Measurements, Unit};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;
use std::collections::BTreeMap;

/// The `[size]` ceilings for one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeLimits {
    pub file_loc: Threshold,
    pub function_loc: Threshold,
    pub complexity: Threshold,
    pub nesting: Threshold,
}

impl SizeLimits {
    pub fn describe(&self, source: &str) -> String {
        format!(
            "{source} [size] file_loc.fail={}, function_loc.fail={}, complexity.fail={}, nesting.fail={}",
            self.file_loc.fail, self.function_loc.fail, self.complexity.fail, self.nesting.fail
        )
    }
}

pub fn size_limits(config: &Config, target: &Target) -> SizeLimits {
    let name = Some(target.name.as_str());
    SizeLimits {
        file_loc: config.threshold("size", "file_loc", Threshold::new(300, 500), name),
        function_loc: config.threshold("size", "function_loc", Threshold::new(40, 60), name),
        complexity: config.threshold("size", "complexity", Threshold::new(10, 15), name),
        nesting: config.threshold("size", "nesting", Threshold::new(3, 4), name),
    }
}

pub fn judge_file_sizes(
    lines: &BTreeMap<String, i64>,
    limits: &SizeLimits,
) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut details = Vec::new();

    for (path, code) in lines {
        if *code >= limits.file_loc.fail {
            problems.push(format!(
                "{path}: {code} code lines (fail >= {})",
                limits.file_loc.fail
            ));
        } else if *code >= limits.file_loc.warn {
            details.push(format!(
                "file {path}: {code} code lines (warn >= {})",
                limits.file_loc.warn
            ));
        } else {
            details.push(format!("file {path}: {code} code lines (ok)"));
        }
    }
    (problems, details)
}

/// The deepest block of each file against the one nesting ceiling — the
/// granularity a rule that counts blocks, and not functions, can report.
pub fn judge_file_nesting(
    depths: &BTreeMap<String, i64>,
    limits: &SizeLimits,
) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut details = Vec::new();

    for (path, depth) in depths {
        if *depth >= limits.nesting.fail {
            problems.push(format!(
                "{path}: nesting depth {depth} (fail >= {})",
                limits.nesting.fail
            ));
        } else if *depth >= limits.nesting.warn {
            details.push(format!(
                "file {path}: nesting depth {depth} (warn >= {})",
                limits.nesting.warn
            ));
        }
    }
    (problems, details)
}

/// `(failure, warning)` for one measured value against its `{warn, fail}` band.
pub fn bound_verdict(
    label: &str,
    unit: &Unit,
    value: i64,
    bound: &Threshold,
) -> (Option<String>, Option<String>) {
    if value >= bound.fail {
        return (
            Some(format!(
                "{}: {} has {label} {value} (fail >= {})",
                unit.path, unit.name, bound.fail
            )),
            None,
        );
    }
    if value >= bound.warn {
        return (None, Some(format!("{label}(warn)")));
    }
    (None, None)
}

pub fn unit_verdict(unit: &Unit, limits: &SizeLimits) -> (Vec<String>, Vec<String>) {
    let bounds: [(&str, Option<i64>, &Threshold); 3] = [
        ("function_loc", Some(unit.sloc), &limits.function_loc),
        ("complexity", Some(unit.cyclomatic), &limits.complexity),
        ("nesting", unit.nesting, &limits.nesting),
    ];

    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    for (label, value, bound) in bounds {
        let Some(value) = value else {
            continue;
        };
        let (failure, warning) = bound_verdict(label, unit, value, bound);
        if let Some(failure) = failure {
            failures.push(failure);
        }
        if let Some(warning) = warning {
            warnings.push(warning);
        }
    }
    (failures, warnings)
}

pub fn judge_function_metrics(units: &[Unit], limits: &SizeLimits) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut near_ceiling = Vec::new();

    for unit in units {
        let (failures, warnings) = unit_verdict(unit, limits);
        problems.extend(failures.iter().cloned());
        if !warnings.is_empty() && failures.is_empty() {
            near_ceiling.push(format!(
                "near the ceiling: {} ({}) {} sloc, cc {} -> {}",
                unit.name,
                unit.path,
                unit.sloc,
                unit.cyclomatic,
                warnings.join(", ")
            ));
        }
    }
    (problems, near_ceiling)
}

pub fn gate_size(run: &GateRun<'_>, measurements: &Measurements) -> GateResult {
    let GateRun {
        target,
        config,
        lang,
        ..
    } = *run;
    let limits = size_limits(config, target);
    let contract = limits.describe(&config.source());
    let tool = config.text_setting("size", "tool", lang.size_tool(), Some(&target.name));

    if let Some(result) = unsupported_tool(&tool, &contract, lang) {
        return result;
    }

    let (problems, mut details) = judgement(measurements, &limits);
    details.extend(evidence_notes(measurements, lang.metrics_tool()));

    let status = size_status(&problems, measurements);
    GateResult::new(
        "size",
        status,
        worst_summary(measurements, lang),
        details.into_iter().chain(problems).collect::<Vec<_>>(),
    )
    .contract(contract)
    .fixes(Gate::Size.fix_hints().iter().copied())
}

/// A `tool` this module cannot drive is INCOMPLETE, never a pass.
fn unsupported_tool(tool: &str, contract: &str, lang: Lang) -> Option<GateResult> {
    let supported = lang.size_tool();
    if tool == supported {
        return None;
    }

    Some(
        GateResult::new(
            "size",
            INCOMPLETE,
            format!("tool `{tool}` is not supported by this module"),
            [format!(
                "`.mido.toml` names `{tool}`; the {} module measures with {supported}",
                lang.name()
            )],
        )
        .contract(contract)
        .fixes([format!(
            "point [size] tool at {supported}, or extend the module with a parser for that tool"
        )]),
    )
}

/// The problems and evidence one measurement pass produced.
fn judgement(measurements: &Measurements, limits: &SizeLimits) -> (Vec<String>, Vec<String>) {
    let (mut problems, mut details) = judge_file_sizes(&measurements.lines, limits);
    let (nesting_problems, nesting_details) = judge_file_nesting(&measurements.nesting, limits);
    problems.extend(nesting_problems);
    details.extend(nesting_details);

    let (unit_problems, unit_details) = judge_function_metrics(&measurements.units, limits);
    problems.extend(unit_problems);
    details.extend(unit_details);
    (problems, details)
}

/// The measurement caveats a passing size gate still has to admit: a tool that
/// could not read a file, metrics that never arrived, nesting that is absent.
fn evidence_notes(measurements: &Measurements, tool: &str) -> Vec<String> {
    let mut notes: Vec<String> = measurements
        .errors
        .iter()
        .map(|error| format!("measurement: {error}"))
        .collect();
    if measurements.units.is_empty() && !measurements.files.is_empty() {
        notes.push(format!("function metrics: {tool} returned nothing"));
    }
    if !measurements.units.is_empty()
        && measurements.nesting.is_empty()
        && measurements.units.iter().all(|unit| unit.nesting.is_none())
    {
        notes.push(format!(
            "nesting: not measured ({tool} exposes no nesting metric for this input)"
        ));
    }
    notes
}

/// A measured problem fails; missing measurements are INCOMPLETE, never a pass.
fn size_status(problems: &[String], measurements: &Measurements) -> &'static str {
    if !problems.is_empty() {
        return FAIL;
    }
    if !measurements.errors.is_empty()
        || (measurements.units.is_empty() && !measurements.files.is_empty())
    {
        return INCOMPLETE;
    }
    PASS
}

fn worst_summary(measurements: &Measurements, lang: Lang) -> String {
    match measurements.units.iter().max_by_key(|unit| unit.sloc) {
        Some(worst) => format!(
            "worst function {} sloc / cc {} ({})",
            worst.sloc, worst.cyclomatic, worst.name
        ),
        None if measurements.files.is_empty() => {
            format!("no {} source files", lang.source_label())
        }
        None => format!("no functions in {} file(s)", measurements.files.len()),
    }
}

#[cfg(test)]
mod tests;
