//! Gate 2 — size and complexity ceilings.

use crate::config::{Config, Threshold};
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::metrics::Unit;
use crate::process::Runner;
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;
use std::collections::BTreeMap;
use std::path::Path;

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

pub fn gate_size(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    units: &[Unit],
    tool_errors: &[String],
) -> GateResult {
    let GateRun {
        repo,
        target,
        changed,
        config,
        lang,
        ..
    } = *run;
    let limits = size_limits(config, target);
    let source_files = existing_source_files(lang, repo, target, changed);
    let mut measurement_errors: Vec<String> = tool_errors.to_vec();

    let lines = measured_lines(runner, run, &source_files, &mut measurement_errors);
    let (problems, mut details) = judgement(units, &limits, &lines);
    details.extend(evidence_notes(
        units,
        lang.metrics_tool(),
        &measurement_errors,
        &source_files,
    ));

    let status = size_status(&problems, &measurement_errors, units, &source_files);
    GateResult::new(
        "size",
        status,
        worst_summary(units, lang),
        details.into_iter().chain(problems).collect::<Vec<_>>(),
    )
    .contract(limits.describe(&config.source()))
    .fixes(Gate::Size.fix_hints().iter().copied())
}

/// The code lines the size tool reports; measurement failures land in
/// `measurement_errors` and measure nothing.
fn measured_lines(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    source_files: &[String],
    measurement_errors: &mut Vec<String>,
) -> BTreeMap<String, i64> {
    let GateRun {
        repo,
        target,
        config,
        lang,
        ..
    } = *run;
    let tool = config.text_setting("size", "tool", lang.size_tool(), Some(&target.name));
    lang.code_lines(
        runner,
        repo,
        target,
        source_files,
        &tool,
        measurement_errors,
    )
}

/// The problems and evidence one measurement pass produced.
fn judgement(
    units: &[Unit],
    limits: &SizeLimits,
    lines: &BTreeMap<String, i64>,
) -> (Vec<String>, Vec<String>) {
    let (file_problems, mut details) = judge_file_sizes(lines, limits);
    let (unit_problems, unit_details) = judge_function_metrics(units, limits);
    details.extend(unit_details);
    let mut problems = file_problems;
    problems.extend(unit_problems);
    (problems, details)
}

/// The changed source files that still exist — a deleted file has nothing to
/// measure, and tokei fails on a path that is not there.
fn existing_source_files(
    lang: Lang,
    repo: &Path,
    target: &Target,
    changed: &[String],
) -> Vec<String> {
    changed
        .iter()
        .filter(|path| lang.is_source(path) && target.dir(repo).join(path).exists())
        .cloned()
        .collect()
}

/// The measurement caveats a passing size gate still has to admit: a tool that
/// could not read a file, metrics that never arrived, nesting that is absent.
fn evidence_notes(
    units: &[Unit],
    tool: &str,
    measurement_errors: &[String],
    source_files: &[String],
) -> Vec<String> {
    let mut notes: Vec<String> = measurement_errors
        .iter()
        .map(|error| format!("measurement: {error}"))
        .collect();
    if units.is_empty() && !source_files.is_empty() {
        notes.push(format!("function metrics: {tool} returned nothing"));
    }
    if !units.is_empty() && units.iter().all(|unit| unit.nesting.is_none()) {
        notes.push(format!(
            "nesting: not measured ({tool} exposes no nesting metric for this input)"
        ));
    }
    notes
}

/// A measured problem fails; missing measurements are INCOMPLETE, never a pass.
fn size_status(
    problems: &[String],
    measurement_errors: &[String],
    units: &[Unit],
    source_files: &[String],
) -> &'static str {
    if !problems.is_empty() {
        return FAIL;
    }
    if !measurement_errors.is_empty() || (units.is_empty() && !source_files.is_empty()) {
        return INCOMPLETE;
    }
    PASS
}

fn worst_summary(units: &[Unit], lang: Lang) -> String {
    match units.iter().max_by_key(|unit| unit.sloc) {
        Some(worst) => format!(
            "worst function {} sloc / cc {} ({})",
            worst.sloc, worst.cyclomatic, worst.name
        ),
        None => format!("no {} changes", lang.source_label()),
    }
}

#[cfg(test)]
mod tests;
