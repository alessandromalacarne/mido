//! Gate 5 — coverage of the changed code.

use crate::gates::{fix_hints, GateRun};
use crate::metrics::{lcov_files, percent, relative_to, touches, LcovStat};
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The argv to run, with `{lcov}` filled in, and the report it should write.
pub fn coverage_report_path(
    argv: &[String],
    repo: &Path,
    target: &Target,
    scratch: &Path,
) -> (Vec<String>, PathBuf) {
    let report = scratch.join(format!("guardrails-lcov-{}.info", target.name));
    let argv: Vec<String> = argv
        .iter()
        .map(|arg| arg.replace("{lcov}", &report.to_string_lossy()))
        .collect();

    let path = output_path(&argv).unwrap_or_else(|| PathBuf::from("lcov.info"));
    let path = if path.is_absolute() {
        path
    } else {
        target.dir(repo).join(path)
    };
    (argv, path)
}

/// Where the command says it writes: `--output-path <path>` or `--output-path=<path>`.
fn output_path(argv: &[String]) -> Option<PathBuf> {
    let mut args = argv.iter();
    while let Some(arg) = args.next() {
        if arg == "--output-path" {
            return args.next().map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--output-path=") {
            return Some(PathBuf::from(value));
        }
    }
    None
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
                "changed file {shown}: {}/{} = {value:.1}% (min {minimum})",
                stats.lines_hit, stats.lines_found
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
                "total coverage dropped {delta:+.1} points (max {drop_max})"
            )],
            details,
        );
    }
    (Vec::new(), details)
}

pub fn gate_coverage(runner: &dyn Runner, run: &GateRun<'_>) -> GateResult {
    let GateRun {
        repo,
        target,
        config,
        lang,
        changed,
        scratch,
        baseline_lcov,
        ..
    } = *run;
    let minimum = config.float("coverage", "changed_file_min", 80.0, Some(&target.name));
    let drop_max = config.float("coverage", "total_drop_max", 0.0, Some(&target.name));
    let argv = config
        .argv("coverage", "command", target)
        .or_else(|| lang.fallback_argv("coverage", "command"))
        .unwrap_or_default();
    let (argv, report_path) = coverage_report_path(&argv, repo, target, scratch);
    let command = argv.join(" ");
    let contract = format!(
        "{} [coverage] changed_file_min={minimum} via `{command}`",
        config.source()
    );

    let result = process::dev(runner, lang.env_tool(), &target.dir(repo), &argv, None);
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
    use crate::config::Config;
    use crate::lang::Lang;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root, &Lang::Rust).expect("config loads")
    }

    fn gate_run_for<'a>(
        repo: &'a MiniRepo,
        config: &'a Config,
        changed: &'a [String],
    ) -> GateRun<'a> {
        let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
        crate::test_support::gate_run(&repo.root, target, config, changed)
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn write_report(path: &Path, body: &str) {
        std::fs::write(path, body).expect("report");
    }

    #[test]
    fn the_report_path_is_filled_in_and_located() {
        let repo = repo();
        let scratch = repo.root.join("scratch");

        let (command, path) = coverage_report_path(
            &argv(&["cargo", "llvm-cov", "--lcov", "--output-path", "{lcov}"]),
            &repo.root,
            &Target::workspace_target("Cargo.toml"),
            &scratch,
        );

        assert!(command.join(" ").contains("guardrails-lcov-workspace.info"));
        assert_eq!(path, scratch.join("guardrails-lcov-workspace.info"));
    }

    #[test]
    fn a_relative_report_path_lands_in_the_target_directory() {
        let repo = repo();

        let (_, path) = coverage_report_path(
            &argv(&["cargo", "llvm-cov", "--lcov", "--output-path", "lcov.info"]),
            &repo.root,
            &Target::crate_target("frontend", false, "Cargo.toml"),
            &repo.root,
        );

        assert_eq!(path, repo.root.join("frontend/lcov.info"));
    }

    #[test]
    fn the_equals_form_of_the_output_flag_is_read_too() {
        let repo = repo();

        let (_, path) = coverage_report_path(
            &argv(&["cargo", "llvm-cov", "--output-path=lcov.info"]),
            &repo.root,
            &Target::workspace_target("Cargo.toml"),
            &repo.root,
        );

        assert_eq!(path, repo.root.join("lcov.info"));
    }

    #[test]
    fn a_command_without_an_output_flag_defaults_to_lcov_info() {
        let repo = repo();

        let (_, path) = coverage_report_path(
            &argv(&["cargo", "llvm-cov"]),
            &repo.root,
            &Target::workspace_target("Cargo.toml"),
            &repo.root,
        );

        assert_eq!(path, repo.root.join("lcov.info"));
    }

    #[test]
    fn coverage_gate_enforces_the_changed_file_minimum() {
        let repo = repo();
        // The embedded rust baseline writes `lcov.info` next to the target.
        write_report(
            &repo.root.join("lcov.info"),
            "SF:/repo/lib/src/foo.rs\nLH:2\nLF:10\nend_of_record\n",
        );
        let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
        let changed = vec!["lib/src/foo.rs".to_string()];
        let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &changed));

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("20.0%"));
        assert!(result.details.join(" ").contains("min 80"));
    }

    #[test]
    fn a_missing_report_is_incomplete_not_a_pass() {
        let repo = repo();
        let runner = FakeRunner::with(&[("llvm-cov", 101, "error: no such command")]);
        let changed = vec!["lib/src/foo.rs".to_string()];
        let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &changed));

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
        write_report(
            &repo.root.join("lcov.info"),
            "SF:/repo/lib/src/other.rs\nLH:10\nLF:10\nend_of_record\n",
        );
        let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
        let changed = vec!["lib/src/foo.rs".to_string()];
        let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &changed));

        assert_eq!(result.status, PASS);
        assert!(result.details.join(" ").contains("unverified, not 100%"));
    }

    #[test]
    fn a_total_drop_beyond_the_allowance_fails_the_gate() {
        let repo = repo();
        write_report(
            &repo.root.join("lcov.info"),
            "SF:/repo/lib/src/foo.rs\nLH:5\nLF:10\nend_of_record\n",
        );
        let baseline = repo.root.join("baseline.info");
        write_report(
            &baseline,
            "SF:/repo/lib/src/foo.rs\nLH:10\nLF:10\nend_of_record\n",
        );
        let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
        let changed = vec!["lib/src/foo.rs".to_string()];
        let config = config_for(&repo);
        let mut run = gate_run_for(&repo, &config, &changed);
        run.baseline_lcov = Some(&baseline);
        let result = gate_coverage(&runner, &run);

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
            &Target::workspace_target("Cargo.toml"),
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
            &Target::crate_target("frontend", false, "Cargo.toml"),
            80.0,
        );

        assert!(problems.is_empty());
        assert!(details[0].starts_with("changed file src/main.rs: 2/2 = 100.0% (ok)"));
    }
}
