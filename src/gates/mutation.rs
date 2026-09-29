//! Gate 6 — do the tests actually detect broken code?

use crate::config::Config;
use crate::gates::{fix_hints, general, RUST_EXT};
use crate::metrics::percent;
use crate::process::last_lines;
use crate::process::{self, Command, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::{covers, Target};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

pub const DEFAULT_MUTANT_TIMEOUT: i64 = 120;

fn summary_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(\d+) mutants tested in [^:]+: (\d+) missed, (\d+) caught, (\d+) unviable")
            .expect("valid pattern")
    })
}

fn git_args(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// The patch cargo-mutants should mutate: the working-tree diff, untracked files included.
fn changed_scope_args(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    patch: &Path,
) -> Result<Vec<String>, std::io::Error> {
    let untracked: Vec<String> = process::git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .lines()
    .filter(|path| path.ends_with(RUST_EXT) && covers(path, target))
    .map(str::to_string)
    .collect();

    if !untracked.is_empty() {
        let mut add = vec!["add".to_string(), "-N".to_string()];
        add.extend(untracked.iter().cloned());
        process::git(runner, repo, &git_args(&add));
    }

    let mut diff = vec!["git".to_string(), "diff".to_string()];
    if !target.path.is_empty() {
        diff.push(format!("--relative={}", target.path));
    }
    let result = runner.exec(&Command::new(repo, diff));
    std::fs::write(patch, &result.stdout)?;

    if !untracked.is_empty() {
        // Leave the index as it was found: intent-to-add entries would otherwise
        // show up as staged in whatever commit comes next.
        let mut reset = vec!["reset".to_string(), "-q".to_string(), "--".to_string()];
        reset.extend(untracked);
        process::git(runner, repo, &git_args(&reset));
    }

    Ok(vec![
        "--in-place".to_string(),
        "--in-diff".to_string(),
        patch.to_string_lossy().to_string(),
    ])
}

pub fn gate_mutation(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    config: &Config,
    scratch: &Path,
) -> GateResult {
    let minimum = config.float("mutation", "kill_rate_min", 70.0, Some(&target.name));
    let timeout = config.int("mutation", "timeout_secs", 3600, Some(&target.name));
    let scope = config.text_setting("mutation", "scope", "changed", Some(&target.name));
    let command = config.command("mutation", "command", target, "cargo mutants");
    let patch = scratch.join(format!("guardrails-changed-{}.patch", target.name));

    let mut args = process::split(&command);
    match scope_args(runner, repo, target, &scope, &patch) {
        Ok(scoped) => args.extend(scoped),
        Err(error) => {
            return GateResult::new(
                "mutation",
                INCOMPLETE,
                "the changed-file patch could not be written",
                [error.to_string()],
            )
            .fixes(fix_hints("mutation").iter().copied());
        }
    }

    args.push("--timeout".to_string());
    args.push(DEFAULT_MUTANT_TIMEOUT.to_string());
    let contract = format!(
        "{} [mutation] kill_rate_min={}, scope={scope} via `{}`",
        config.source(),
        general(minimum),
        args.join(" ")
    );

    let result = process::dev(
        runner,
        repo,
        &target.dir(repo),
        &args,
        Some(timeout.max(0) as u64),
    );
    judge_mutation(&result, &contract, minimum, timeout)
}

/// `cargo mutants` in place, either against the changed-file patch or a named path.
fn scope_args(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    scope: &str,
    patch: &Path,
) -> Result<Vec<String>, std::io::Error> {
    if scope == "changed" {
        return changed_scope_args(runner, repo, target, patch);
    }
    if scope.is_empty() || scope == "all" {
        return Ok(vec!["--in-place".to_string()]);
    }
    Ok(vec![
        "--in-place".to_string(),
        "--file".to_string(),
        scope.to_string(),
    ])
}

fn judge_mutation(
    result: &process::Outcome,
    contract: &str,
    minimum: f64,
    timeout: i64,
) -> GateResult {
    let output = result.combined();

    if result.code == process::TIMEOUT_EXIT {
        return GateResult::new(
            "mutation",
            INCOMPLETE,
            format!("timed out after {timeout}s"),
            tail(&output),
        )
        .contract(contract)
        .fixes(fix_hints("mutation").iter().copied());
    }

    let Some(summary) = summary_pattern().captures(&output) else {
        return GateResult::new(
            "mutation",
            INCOMPLETE,
            "cargo-mutants produced no summary",
            tail(&output),
        )
        .contract(contract)
        .fixes(fix_hints("mutation").iter().copied());
    };

    let total: i64 = summary[1].parse().unwrap_or_default();
    let missed: i64 = summary[2].parse().unwrap_or_default();
    let caught: i64 = summary[3].parse().unwrap_or_default();
    let unviable: i64 = summary[4].parse().unwrap_or_default();
    let rate = percent(caught, caught + missed);

    let mut details = vec![format!(
        "{total} mutants: {caught} caught, {missed} missed, {unviable} unviable -> {rate:.1}% killed"
    )];
    details.extend(
        output
            .lines()
            .filter(|line| line.starts_with("MISSED"))
            .map(|line| line.trim().to_string()),
    );

    let status = if rate < minimum { FAIL } else { PASS };
    GateResult::new(
        "mutation",
        status,
        format!("{rate:.1}% killed (min {})", general(minimum)),
        details,
    )
    .contract(contract)
    .fixes(fix_hints("mutation").iter().copied())
}

fn tail(output: &str) -> Vec<String> {
    last_lines(output, 10)
}
