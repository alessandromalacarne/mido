//! Gate 2 — size and complexity ceilings.

use crate::config::{Config, Threshold};
use crate::gates::{fix_hints, RUST_EXT};
use crate::metrics::Unit;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;
use serde_json::Value as Json;
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

pub fn tokei_code_lines(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    tool: &str,
    on_error: &mut Vec<String>,
) -> BTreeMap<String, i64> {
    if files.is_empty() {
        return BTreeMap::new();
    }
    if tool != "tokei" {
        on_error.push(format!(
            "size tool `{tool}` is not supported by this runner (only tokei)"
        ));
        return BTreeMap::new();
    }

    let args: Vec<String> = ["tokei", "--output", "json"]
        .iter()
        .map(|arg| arg.to_string())
        .chain(files.iter().cloned())
        .collect();
    let result = process::dev(runner, repo, &target.dir(repo), &args, None);
    if !result.ok() || result.stdout.trim().is_empty() {
        on_error.push(format!(
            "tokei could not measure {} file(s) (exit {})",
            files.len(),
            result.code
        ));
        return BTreeMap::new();
    }

    let Ok(document) = serde_json::from_str::<Json>(&result.stdout) else {
        on_error.push("tokei printed no usable json".to_string());
        return BTreeMap::new();
    };
    tokei_counts(&document)
}

/// `{language: {reports: [{name, stats: {code}}]}}`, minus the `Total` pseudo-language.
fn tokei_counts(document: &Json) -> BTreeMap<String, i64> {
    let mut counts = BTreeMap::new();
    let Some(languages) = document.as_object() else {
        return BTreeMap::new();
    };

    for (language, info) in languages {
        if language == "Total" {
            continue;
        }
        for report in info
            .get("reports")
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let Some(name) = report.get("name").and_then(Json::as_str) else {
                continue;
            };
            let code = report
                .get("stats")
                .and_then(|stats| stats.get("code"))
                .and_then(Json::as_i64)
                .unwrap_or_default();
            counts.insert(name.to_string(), code);
        }
    }
    counts
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
    repo: &Path,
    target: &Target,
    changed: &[String],
    config: &Config,
    units: &[Unit],
    tool_errors: &[String],
) -> GateResult {
    let limits = size_limits(config, target);
    let rust_files: Vec<String> = changed
        .iter()
        .filter(|path| path.ends_with(RUST_EXT))
        .cloned()
        .collect();
    let mut measurement_errors: Vec<String> = tool_errors.to_vec();

    let lines = tokei_code_lines(
        runner,
        repo,
        target,
        &rust_files,
        &config.command("size", "tool", target, "tokei"),
        &mut measurement_errors,
    );
    let (file_problems, mut details) = judge_file_sizes(&lines, &limits);
    let (unit_problems, unit_details) = judge_function_metrics(units, &limits);
    details.extend(unit_details);
    let mut problems: Vec<String> = file_problems;
    problems.extend(unit_problems);

    details.extend(
        measurement_errors
            .iter()
            .map(|error| format!("measurement: {error}")),
    );
    if units.is_empty() && !rust_files.is_empty() {
        details.push("function metrics: rust-code-analysis returned nothing".to_string());
    }
    if !units.is_empty() && units.iter().all(|unit| unit.nesting.is_none()) {
        details.push(
            "nesting: not measured (rust-code-analysis exposes no nesting metric for this input)"
                .to_string(),
        );
    }

    let status = size_status(&problems, &measurement_errors, units, &rust_files);
    GateResult::new(
        "size",
        status,
        worst_summary(units),
        details.into_iter().chain(problems).collect::<Vec<_>>(),
    )
    .contract(limits.describe(&config.source()))
    .fixes(fix_hints("size").iter().copied())
}

/// A measured problem fails; missing measurements are INCOMPLETE, never a pass.
fn size_status(
    problems: &[String],
    measurement_errors: &[String],
    units: &[Unit],
    rust_files: &[String],
) -> &'static str {
    if !problems.is_empty() {
        return FAIL;
    }
    if !measurement_errors.is_empty() || (units.is_empty() && !rust_files.is_empty()) {
        return INCOMPLETE;
    }
    PASS
}

fn worst_summary(units: &[Unit]) -> String {
    match units.iter().max_by_key(|unit| unit.sloc) {
        Some(worst) => format!(
            "worst function {} sloc / cc {} ({})",
            worst.sloc, worst.cyclomatic, worst.name
        ),
        None => "no rust changes".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn limits() -> SizeLimits {
        SizeLimits {
            file_loc: Threshold::new(300, 500),
            function_loc: Threshold::new(40, 60),
            complexity: Threshold::new(10, 15),
            nesting: Threshold::new(3, 4),
        }
    }

    fn unit(name: &str, sloc: i64) -> Unit {
        Unit {
            sloc,
            name: name.to_string(),
            path: "src/foo.rs".to_string(),
            ..Unit::new("src/foo.rs", name)
        }
    }

    #[test]
    fn file_sizes_fail_at_the_ceiling_and_warn_before_it() {
        let lines = BTreeMap::from([
            ("src/big.rs".to_string(), 500),
            ("src/medium.rs".to_string(), 300),
            ("src/small.rs".to_string(), 10),
        ]);

        let (problems, details) = judge_file_sizes(&lines, &limits());

        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("src/big.rs: 500 code lines (fail >= 500)"));
        assert!(details
            .iter()
            .any(|line| line.contains("src/medium.rs: 300 code lines (warn >= 300)")));
        assert!(details
            .iter()
            .any(|line| line.contains("src/small.rs: 10 code lines (ok)")));
    }

    #[test]
    fn function_metrics_fail_on_length_complexity_and_nesting() {
        let long = unit("long", 61);
        let complex = Unit {
            cyclomatic: 15,
            ..unit("complex", 10)
        };
        let nested = Unit {
            nesting: Some(4),
            ..unit("nested", 10)
        };

        let (problems, near) = judge_function_metrics(&[long, complex, nested], &limits());

        assert_eq!(problems.len(), 3);
        assert!(problems
            .iter()
            .any(|line| line.contains("function_loc 61 (fail >= 60)")));
        assert!(problems
            .iter()
            .any(|line| line.contains("complexity 15 (fail >= 15)")));
        assert!(problems
            .iter()
            .any(|line| line.contains("nesting 4 (fail >= 4)")));
        assert!(near.is_empty());
    }

    #[test]
    fn a_near_ceiling_function_warns_without_failing() {
        let (problems, near) = judge_function_metrics(&[unit("borderline", 45)], &limits());

        assert!(problems.is_empty());
        assert_eq!(near.len(), 1);
        assert!(near[0].contains("near the ceiling: borderline"));
        assert!(near[0].contains("function_loc(warn)"));
    }

    #[test]
    fn an_unmeasured_nesting_does_not_judge_nesting() {
        let (problems, _) = judge_function_metrics(&[unit("no-nesting-data", 10)], &limits());

        assert!(problems.is_empty());
    }

    #[test]
    fn a_function_over_two_ceilings_reports_both_but_is_not_near_one() {
        let over = Unit {
            cyclomatic: 20,
            ..unit("over", 70)
        };

        let (problems, near) = judge_function_metrics(&[over], &limits());

        assert_eq!(problems.len(), 2);
        assert!(near.is_empty());
    }

    #[test]
    fn tokei_counts_come_from_the_report() {
        let repo = MiniRepo::build(None);
        let tokei = serde_json::json!({
            "Rust": {
                "code": 12,
                "reports": [
                    { "name": "src/foo.rs", "stats": { "code": 12 } }
                ]
            },
            "Total": { "code": 12, "reports": [{ "name": "Total", "stats": { "code": 12 } }] }
        });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
        let mut errors = Vec::new();

        let counts = tokei_code_lines(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            "tokei",
            &mut errors,
        );

        assert_eq!(counts.get("src/foo.rs"), Some(&12));
        assert!(!counts.contains_key("Total"));
        assert!(errors.is_empty());
    }

    #[test]
    fn an_unsupported_size_tool_is_an_error_not_a_silent_pass() {
        let repo = MiniRepo::build(None);
        let runner = FakeRunner::default();
        let mut errors = Vec::new();

        let counts = tokei_code_lines(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            "scc",
            &mut errors,
        );

        assert!(counts.is_empty());
        assert!(errors[0].contains("`scc` is not supported"));
    }

    #[test]
    fn unreadable_tokei_output_is_an_error() {
        let repo = MiniRepo::build(None);
        let runner = FakeRunner::with(&[("tokei", 0, "not json")]);
        let mut errors = Vec::new();

        tokei_code_lines(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            "tokei",
            &mut errors,
        );

        assert!(errors.iter().any(|error| error.contains("no usable json")));
    }

    #[test]
    fn the_gate_fails_when_a_changed_file_is_over_the_ceiling() {
        let repo = MiniRepo::build(None);
        let config = Config::load(&repo.root).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 600 } }] } });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
        let units = vec![unit("small", 5)];

        let result = gate_size(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &config,
            &units,
            &[],
        );

        assert_eq!(result.status, FAIL);
        assert!(result.summary.contains("worst function"));
        assert!(result.contract.contains("file_loc.fail=500"));
    }

    #[test]
    fn the_gate_is_incomplete_when_the_metrics_never_arrived() {
        let repo = MiniRepo::build(None);
        let config = Config::load(&repo.root).expect("config loads");
        let runner = FakeRunner::with(&[("tokei", 127, "")]);

        let result = gate_size(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &config,
            &[],
            &[],
        );

        assert_eq!(result.status, INCOMPLETE);
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("measurement:")));
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("returned nothing")));
    }

    #[test]
    fn a_change_without_rust_files_passes_as_nothing_to_measure() {
        let repo = MiniRepo::build(None);
        let config = Config::load(&repo.root).expect("config loads");
        let runner = FakeRunner::default();

        let result = gate_size(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["README.md".to_string()],
            &config,
            &[],
            &[],
        );

        assert_eq!(result.status, PASS);
        assert_eq!(result.summary, "no rust changes");
    }

    #[test]
    fn nesting_absence_is_stated_in_the_evidence() {
        let repo = MiniRepo::build(None);
        let config = Config::load(&repo.root).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);

        let result = gate_size(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &config,
            &[unit("f", 5)],
            &[],
        );

        assert_eq!(result.status, PASS);
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("nesting: not measured")));
    }
}
