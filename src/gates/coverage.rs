//! Gate 5 — coverage of the changed code.

use crate::gate::Gate;
use crate::gates::GateRun;
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
    let totals = totals(&files);
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

    coverage_verdict(totals, problems, details, contract)
}

/// Line totals over every file in the report.
fn totals(files: &BTreeMap<String, LcovStat>) -> (i64, i64) {
    (
        files.values().map(|stats| stats.lines_hit).sum(),
        files.values().map(|stats| stats.lines_found).sum(),
    )
}

/// The verdict once the problems and evidence are gathered.
fn coverage_verdict(
    totals: (i64, i64),
    problems: Vec<String>,
    mut details: Vec<String>,
    contract: String,
) -> GateResult {
    let status = if problems.is_empty() { PASS } else { FAIL };
    details.extend(problems);

    GateResult::new(
        "coverage",
        status,
        format!("total {:.1}%", percent(totals.0, totals.1)),
        details,
    )
    .contract(contract)
    .fixes(Gate::Coverage.fix_hints().iter().copied())
}

#[cfg(test)]
mod tests;
