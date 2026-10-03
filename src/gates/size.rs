//! Gate 2 — size and complexity ceilings.

use crate::config::{Config, Threshold};
use crate::gates::{fix_hints, GateRun};
use crate::lang::Lang;
use crate::metrics::{tokei_code_lines, Unit};
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

    let lines = tokei_code_lines(
        runner,
        repo,
        target,
        &source_files,
        &config.text_setting("size", "tool", "tokei", Some(&target.name)),
        lang,
        &mut measurement_errors,
    );
    let (file_problems, mut details) = judge_file_sizes(&lines, &limits);
    let (unit_problems, unit_details) = judge_function_metrics(units, &limits);
    details.extend(unit_details);
    let mut problems: Vec<String> = file_problems;
    problems.extend(unit_problems);

    let tool = lang.analysis_supported_tool();
    details.extend(evidence_notes(
        units,
        tool,
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
    .fixes(fix_hints("size").iter().copied())
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
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::lang::Lang;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn gate_run_for<'a>(
        repo: &'a MiniRepo,
        config: &'a Config,
        changed: &'a [String],
    ) -> GateRun<'a> {
        let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
        crate::test_support::gate_run(&repo.root, target, config, changed)
    }

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

    fn touch(repo: &MiniRepo, path: &str) {
        let full = repo.root.join(path);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("dir");
        std::fs::write(&full, "").expect("file");
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
    fn the_gate_fails_when_a_changed_file_is_over_the_ceiling() {
        let repo = MiniRepo::build(None);
        touch(&repo, "src/foo.rs");
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 600 } }] } });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
        let units = vec![unit("small", 5)];

        let changed = vec!["src/foo.rs".to_string()];
        let result = gate_size(
            &runner,
            &gate_run_for(&repo, &config, &changed),
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
        touch(&repo, "src/foo.rs");
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        let runner = FakeRunner::with(&[("tokei", 127, "")]);

        let changed = vec!["src/foo.rs".to_string()];
        let result = gate_size(&runner, &gate_run_for(&repo, &config, &changed), &[], &[]);

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
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        let runner = FakeRunner::default();

        let changed = vec!["README.md".to_string()];
        let result = gate_size(&runner, &gate_run_for(&repo, &config, &changed), &[], &[]);

        assert_eq!(result.status, PASS);
        assert_eq!(result.summary, "no rust changes");
        assert!(
            !result
                .details
                .iter()
                .any(|line| line.contains("returned nothing")),
            "nothing was measured, so nothing can be reported missing: {:?}",
            result.details
        );
    }

    #[test]
    fn a_measurement_error_with_units_present_is_still_incomplete() {
        let repo = MiniRepo::build(None);
        touch(&repo, "src/foo.rs");
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
        let errors = vec!["src/bar.rs: rust-code-analysis could not read it".to_string()];

        let changed = vec!["src/foo.rs".to_string()];
        let result = gate_size(
            &runner,
            &gate_run_for(&repo, &config, &changed),
            &[unit("f", 5)],
            &errors,
        );

        assert_eq!(result.status, INCOMPLETE);
    }

    #[test]
    fn missing_units_do_not_claim_nesting_was_absent() {
        let repo = MiniRepo::build(None);
        touch(&repo, "src/foo.rs");
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);

        let changed = vec!["src/foo.rs".to_string()];
        let result = gate_size(&runner, &gate_run_for(&repo, &config, &changed), &[], &[]);

        assert!(result
            .details
            .iter()
            .any(|line| line.contains("returned nothing")));
        assert!(
            !result
                .details
                .iter()
                .any(|line| line.contains("nesting: not measured")),
            "no units arrived, so nesting was never looked at: {:?}",
            result.details
        );
    }

    #[test]
    fn nesting_absence_is_stated_in_the_evidence() {
        let repo = MiniRepo::build(None);
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
        let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);

        let changed = vec!["src/foo.rs".to_string()];
        let result = gate_size(
            &runner,
            &gate_run_for(&repo, &config, &changed),
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
