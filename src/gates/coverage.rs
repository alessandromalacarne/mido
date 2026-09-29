//! Gate 5 — coverage of the changed code.

use crate::config::Config;
use crate::gates::{fix_hints, general};
use crate::metrics::{lcov_files, percent, relative_to, touches, LcovStat};
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;
use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn output_path_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"--output-path[= ](\S+)").expect("valid pattern"))
}

/// The command to run, with `{lcov}` filled in, and the report it should write.
pub fn coverage_report_path(
    command: &str,
    repo: &Path,
    target: &Target,
    scratch: &Path,
) -> (String, PathBuf) {
    let report = scratch.join(format!("guardrails-lcov-{}.info", target.name));
    let command = command.replace("{lcov}", &report.to_string_lossy());

    let path = match output_path_pattern().captures(&command) {
        Some(captures) => PathBuf::from(&captures[1]),
        None => PathBuf::from("lcov.info"),
    };
    let path = if path.is_absolute() {
        path
    } else {
        target.dir(repo).join(path)
    };
    (command, path)
}

pub fn judge_changed_coverage(
    files: &BTreeMap<String, LcovStat>,
    changed: &[String],
    target: &Target,
    minimum: f64,
) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut details = Vec::new();
    let mut seen_changed = false;

    for (path, stats) in files {
        if !touches(path, changed) {
            continue;
        }
        seen_changed = true;
        let value = percent(stats.lines_hit, stats.lines_found);
        let verdict = if value >= minimum { "ok" } else { "FAIL" };
        let shown = relative_to(path, target);
        details.push(format!(
            "changed file {shown}: {}/{} = {value:.1}% ({verdict})",
            stats.lines_hit, stats.lines_found
        ));
        if value < minimum {
            problems.push(format!(
                "changed file {shown}: {}/{} = {value:.1}% (min {})",
                stats.lines_hit,
                stats.lines_found,
                general(minimum)
            ));
        }
    }

    if !seen_changed {
        details.push(
            "no changed file appears in the report — coverage of this change is unverified, not 100%"
                .to_string(),
        );
    }
    (problems, details)
}

pub fn judge_coverage_delta(
    totals: (i64, i64),
    baseline_lcov: Option<&Path>,
    drop_max: f64,
) -> (Vec<String>, Vec<String>) {
    let Some(baseline) = baseline_lcov.filter(|path| path.exists()) else {
        return (
            Vec::new(),
            vec!["baseline total: not measured (pass --baseline-lcov to compare)".to_string()],
        );
    };

    let base_files = lcov_files(baseline);
    let base_hit: i64 = base_files.values().map(|stats| stats.lines_hit).sum();
    let base_found: i64 = base_files.values().map(|stats| stats.lines_found).sum();
    let delta = percent(totals.0, totals.1) - percent(base_hit, base_found);
    let details = vec![format!(
        "baseline total: {:.1}% -> delta {delta:+.1} points",
        percent(base_hit, base_found)
    )];

    if delta < -drop_max.abs() {
        return (
            vec![format!(
                "total coverage dropped {delta:+.1} points (max {})",
                general(drop_max)
            )],
            details,
        );
    }
    (Vec::new(), details)
}

pub fn gate_coverage(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    config: &Config,
    changed: &[String],
    baseline_lcov: Option<&Path>,
    scratch: &Path,
) -> GateResult {
    let minimum = config.float("coverage", "changed_file_min", 80.0, Some(&target.name));
    let drop_max = config.float("coverage", "total_drop_max", 0.0, Some(&target.name));
    let (command, report_path) = coverage_report_path(
        &config.command(
            "coverage",
            "command",
            target,
            "cargo llvm-cov --lcov --output-path {lcov}",
        ),
        repo,
        target,
        scratch,
    );
    let contract = format!(
        "{} [coverage] changed_file_min={} via `{command}`",
        config.source(),
        general(minimum)
    );

    let result = process::dev(
        runner,
        repo,
        &target.dir(repo),
        &process::split(&command),
        None,
    );
    if !result.ok() || !report_path.exists() {
        return missing_report(&result, &command, &report_path, &contract);
    }

    judge_coverage(
        &report_path,
        changed,
        target,
        baseline_lcov,
        minimum,
        drop_max,
        contract,
    )
}

/// No report means no evidence: INCOMPLETE, with the tool's own tail as the reason.
fn missing_report(
    result: &process::Outcome,
    command: &str,
    report_path: &Path,
    contract: &str,
) -> GateResult {
    let stream = if result.stderr.trim().is_empty() {
        &result.stdout
    } else {
        &result.stderr
    };
    let mut details = vec![
        format!("`{command}` exited {}", result.code),
        format!("expected report at {}", report_path.display()),
    ];
    details.extend(process::last_lines(stream, 10));

    GateResult::new("coverage", INCOMPLETE, "no usable coverage report", details)
        .contract(contract)
        .fixes([format!(
            "check that `{command}` writes {}",
            report_path.display()
        )])
}

fn judge_coverage(
    report_path: &Path,
    changed: &[String],
    target: &Target,
    baseline_lcov: Option<&Path>,
    minimum: f64,
    drop_max: f64,
    contract: String,
) -> GateResult {
    let files = lcov_files(report_path);
    let totals = (
        files.values().map(|stats| stats.lines_hit).sum::<i64>(),
        files.values().map(|stats| stats.lines_found).sum::<i64>(),
    );
    let (mut problems, mut details) = judge_changed_coverage(&files, changed, target, minimum);
    let (delta_problems, delta_details) = judge_coverage_delta(totals, baseline_lcov, drop_max);
    problems.extend(delta_problems);

    details.insert(0, format!("report: {}", report_path.display()));
    details.insert(
        1,
        format!(
            "total: {}/{} = {:.1}%",
            totals.0,
            totals.1,
            percent(totals.0, totals.1)
        ),
    );
    details.extend(delta_details);

    let status = if problems.is_empty() { PASS } else { FAIL };
    details.extend(problems);

    GateResult::new(
        "coverage",
        status,
        format!("total {:.1}%", percent(totals.0, totals.1)),
        details,
    )
    .contract(contract)
    .fixes(fix_hints("coverage").iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root).expect("config loads")
    }

    fn write_report(path: &Path, body: &str) {
        std::fs::write(path, body).expect("report");
    }

    #[test]
    fn the_report_path_is_filled_in_and_located() {
        let repo = repo();
        let scratch = repo.root.join("scratch");

        let (command, path) = coverage_report_path(
            "cargo llvm-cov --lcov --output-path {lcov}",
            &repo.root,
            &Target::workspace_target(),
            &scratch,
        );

        assert!(command.contains("guardrails-lcov-workspace.info"));
        assert_eq!(path, scratch.join("guardrails-lcov-workspace.info"));
    }

    #[test]
    fn a_relative_report_path_lands_in_the_target_directory() {
        let repo = repo();

        let (_, path) = coverage_report_path(
            "cargo llvm-cov --lcov --output-path lcov.info",
            &repo.root,
            &Target::crate_target("frontend", false),
            &repo.root,
        );

        assert_eq!(path, repo.root.join("frontend/lcov.info"));
    }

    #[test]
    fn a_command_without_an_output_flag_defaults_to_lcov_info() {
        let repo = repo();

        let (_, path) = coverage_report_path(
            "cargo llvm-cov",
            &repo.root,
            &Target::workspace_target(),
            &repo.root,
        );

        assert_eq!(path, repo.root.join("lcov.info"));
    }

    #[test]
    fn coverage_gate_enforces_the_changed_file_minimum() {
        let repo = repo();
        let scratch = repo.root.join("scratch");
        std::fs::create_dir_all(&scratch).expect("scratch");
        write_report(
            &scratch.join("guardrails-lcov-workspace.info"),
            "SF:/repo/lib/src/foo.rs\nLH:2\nLF:10\nend_of_record\n",
        );
        let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);

        let result = gate_coverage(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &["lib/src/foo.rs".to_string()],
            None,
            &scratch,
        );

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("20.0%"));
        assert!(result.details.join(" ").contains("min 80"));
    }

    #[test]
    fn a_missing_report_is_incomplete_not_a_pass() {
        let repo = repo();
        let scratch = repo.root.join("scratch");
        std::fs::create_dir_all(&scratch).expect("scratch");
        let runner = FakeRunner::with(&[("llvm-cov", 101, "error: no such command")]);

        let result = gate_coverage(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &["lib/src/foo.rs".to_string()],
            None,
            &scratch,
        );

        assert_eq!(result.status, INCOMPLETE);
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("exited 101")));
        assert!(result.fixes[0].contains("writes"));
    }

    #[test]
    fn changed_files_absent_from_the_report_are_called_unverified() {
        let repo = repo();
        let scratch = repo.root.join("scratch");
        std::fs::create_dir_all(&scratch).expect("scratch");
        write_report(
            &scratch.join("guardrails-lcov-workspace.info"),
            "SF:/repo/lib/src/other.rs\nLH:10\nLF:10\nend_of_record\n",
        );
        let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);

        let result = gate_coverage(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &["lib/src/foo.rs".to_string()],
            None,
            &scratch,
        );

        assert_eq!(result.status, PASS);
        assert!(result.details.join(" ").contains("unverified, not 100%"));
    }

    #[test]
    fn a_total_drop_beyond_the_allowance_fails_the_gate() {
        let repo = repo();
        let scratch = repo.root.join("scratch");
        std::fs::create_dir_all(&scratch).expect("scratch");
        write_report(
            &scratch.join("guardrails-lcov-workspace.info"),
            "SF:/repo/lib/src/foo.rs\nLH:5\nLF:10\nend_of_record\n",
        );
        let baseline = repo.root.join("baseline.info");
        write_report(
            &baseline,
            "SF:/repo/lib/src/foo.rs\nLH:10\nLF:10\nend_of_record\n",
        );
        let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);

        let result = gate_coverage(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &["lib/src/foo.rs".to_string()],
            Some(&baseline),
            &scratch,
        );

        assert_eq!(result.status, FAIL);
        assert!(result
            .details
            .join(" ")
            .contains("coverage dropped -50.0 points"));
    }

    #[test]
    fn without_a_baseline_the_delta_is_reported_as_not_measured() {
        let (problems, details) = judge_coverage_delta((0, 0), None, 0.0);

        assert!(problems.is_empty());
        assert!(details[0].contains("not measured"));
    }

    #[test]
    fn a_file_missing_from_the_changed_list_is_not_judged() {
        let files = BTreeMap::from([(
            "/repo/lib/src/other.rs".to_string(),
            LcovStat {
                lines_found: 10,
                lines_hit: 1,
            },
        )]);

        let (problems, details) = judge_changed_coverage(
            &files,
            &["lib/src/foo.rs".to_string()],
            &Target::workspace_target(),
            80.0,
        );

        assert!(problems.is_empty());
        assert_eq!(details.len(), 1);
    }

    #[test]
    fn changed_files_are_shown_relative_to_the_target() {
        let files = BTreeMap::from([(
            "/repo/frontend/src/main.rs".to_string(),
            LcovStat {
                lines_found: 2,
                lines_hit: 2,
            },
        )]);

        let (problems, details) = judge_changed_coverage(
            &files,
            &["frontend/src/main.rs".to_string()],
            &Target::crate_target("frontend", false),
            80.0,
        );

        assert!(problems.is_empty());
        assert!(details[0].starts_with("changed file src/main.rs: 2/2 = 100.0% (ok)"));
    }
}
