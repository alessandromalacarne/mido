//! Gate 4 — the full suite.

use crate::config::{value, Config};
use crate::gates::fix_hints;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, PASS};
use crate::targets::Target;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

fn summary_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^test result: (\w+)\. (\d+) passed; (\d+) failed").expect("valid pattern")
    })
}

fn failure_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^\s*(?:test\s+)?([A-Za-z0-9_:]+) \.\.\. FAILED").expect("valid pattern")
    })
}

/// The commands the tests gate runs for this target.
///
/// An explicit `[targets.<name>.tests] command` wins, then the root `[tests]
/// command` for the workspace and its members. A standalone crate derives its
/// own — including the wasm32 run when it browser-tests through
/// `wasm-bindgen-test`.
pub fn test_commands(config: &Config, target: &Target, repo: &Path) -> Vec<String> {
    let configured = config.commands("tests", "command", target, Vec::new());
    if !configured.is_empty() {
        return configured;
    }

    let mut commands = vec!["cargo test".to_string()];
    if !target.workspace_member && target.manifest.is_some() && uses_wasm_bindgen_test(repo, target)
    {
        commands.push("cargo test --target wasm32-unknown-unknown".to_string());
    }
    commands
}

pub fn uses_wasm_bindgen_test(repo: &Path, target: &Target) -> bool {
    let Some(manifest) = &target.manifest else {
        return false;
    };
    let path = repo.join(manifest);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(data) = text.parse::<toml::Table>() else {
        return false;
    };
    data.get("dev-dependencies")
        .and_then(value::as_table)
        .map(|dependencies| dependencies.contains_key("wasm-bindgen-test"))
        .unwrap_or(false)
}

pub fn gate_tests(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    config: &Config,
) -> GateResult {
    let timeout = config
        .int("tests", "timeout_secs", 900, Some(&target.name))
        .max(0) as u64;
    let commands = test_commands(config, target, repo);
    let contract = format!("{} [tests] {}", config.source(), commands.join(" && "));

    let mut details: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut status = PASS;

    for command in &commands {
        let verdict = run_command(runner, repo, target, command, timeout);
        details.extend(verdict.details);
        problems.extend(verdict.problems);
        if verdict.failed {
            status = FAIL;
        }
    }

    let summary = if details.is_empty() {
        "no test command".to_string()
    } else {
        details.join("; ")
    };
    details.extend(problems);

    GateResult::new("tests", status, summary, details)
        .contract(contract)
        .fixes(fix_hints("tests").iter().copied())
}

struct CommandVerdict {
    details: Vec<String>,
    problems: Vec<String>,
    failed: bool,
}

fn run_command(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    command: &str,
    timeout: u64,
) -> CommandVerdict {
    let result = process::dev(
        runner,
        repo,
        &target.dir(repo),
        &process::split(command),
        Some(timeout),
    );
    let output = result.combined();
    let reported = summaries(&output);

    if reported.is_empty() {
        return CommandVerdict {
            details: tail_details(&output),
            problems: vec![format!(
                "`{command}` printed no test summary (exit {})",
                result.code
            )],
            failed: true,
        };
    }

    let passed: i64 = reported.iter().map(|(_, passed, _)| passed).sum();
    let failed: i64 = reported.iter().map(|(_, _, failed)| failed).sum();
    let broken = failed > 0 || !result.ok();
    let mut problems = Vec::new();
    if broken {
        problems.push(format!(
            "`{command}`: {failed} failed{}",
            failed_names(&output)
        ));
    }

    CommandVerdict {
        details: vec![format!("{command}: {passed} passed, {failed} failed")],
        problems,
        failed: broken,
    }
}

/// Every `test result:` line, as `(status, passed, failed)`.
fn summaries(output: &str) -> Vec<(String, i64, i64)> {
    summary_pattern()
        .captures_iter(output)
        .map(|captures| {
            (
                captures[1].to_string(),
                captures[2].parse().unwrap_or_default(),
                captures[3].parse().unwrap_or_default(),
            )
        })
        .collect()
}

fn failed_names(output: &str) -> String {
    let mut names: Vec<String> = failure_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .collect();
    names.sort();
    names.dedup();

    if names.is_empty() {
        return String::new();
    }
    format!(
        " -> {}",
        names
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn tail_details(output: &str) -> Vec<String> {
    let lines: Vec<&str> = output.trim().lines().collect();
    let start = lines.len().saturating_sub(10);
    lines[start..]
        .iter()
        .map(|line| format!("  {}", line.trim()))
        .collect()
}
