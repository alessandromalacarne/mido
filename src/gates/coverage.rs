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
