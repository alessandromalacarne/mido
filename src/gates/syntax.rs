//! Gate 1 — formatter, linter, type checker.

use crate::config::{value, Config};
use crate::gates::diagnostics::{dedupe_problems, diagnostic_step, format_step, StepOutcome};
use crate::gates::fix_hints;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, PASS};
use crate::targets::Target;
use std::path::Path;
use toml::{Table, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Command(String),
    Unusable(Value),
}

pub fn syntax_steps(section: &Table, config: &Config, target: &Target) -> Vec<(String, Step)> {
    let mut steps = Vec::new();

    if let Some(configured) = section.get("command").filter(|value| truthy(value)) {
        let step = match configured {
            Value::String(command) => Step::Command(command.clone()),
            other => Step::Unusable(other.clone()),
        };
        steps.push(("command".to_string(), step));
    }

    for (name, key, default) in [
        ("format", "format", "cargo fmt --check"),
        ("lint", "lint", "cargo clippy --all-targets -- -D warnings"),
        ("typecheck", "typecheck", "cargo check"),
    ] {
        steps.push((
            name.to_string(),
            Step::Command(config.command("syntax", key, target, default)),
        ));
    }
    steps
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Boolean(flag) => *flag,
        Value::Integer(number) => *number != 0,
        Value::Float(number) => *number != 0.0,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Table(entries) => !entries.is_empty(),
        Value::Datetime(_) => true,
    }
}

pub fn step_text(step: &Step) -> String {
    match step {
        Step::Command(command) => command.clone(),
        Step::Unusable(value) => value::render(value),
    }
}

pub fn gate_syntax(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    changed: &[String],
    config: &Config,
) -> GateResult {
    let section = config.section("syntax", Some(&target.name));
    let steps = syntax_steps(&section, config, target);
    let contract = format!(
        "{} [syntax] {}",
        config.source(),
        steps
            .iter()
            .map(|(name, step)| format!("{name}={}", process::quote(&step_text(step))))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let timeout = config
        .int("syntax", "timeout_secs", 1800, Some(&target.name))
        .max(0) as u64;

    let mut details: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut crate_wide_debt = false;

    for (name, step) in &steps {
        let Some(command) = usable_command(name, step, &mut details, &mut problems) else {
            continue;
        };
        let outcome = run_step(runner, repo, target, changed, name, &command, timeout);
        details.extend(outcome.details);
        problems.extend(outcome.problems);
        crate_wide_debt = crate_wide_debt || outcome.crate_wide_debt;
    }

    syntax_result(contract, crate_wide_debt, details, problems)
}

/// The step's command, or the two lines that say why it cannot be run.
fn usable_command(
    name: &str,
    step: &Step,
    details: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> Option<String> {
    match step {
        Step::Command(command) => Some(command.clone()),
        Step::Unusable(unusable) => {
            details.push(format!(
                "{name}: command must be a string, got {}",
                value::type_name(unusable)
            ));
            problems.push(format!("{name}: unusable command in [syntax]"));
            None
        }
    }
}

fn run_step(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    changed: &[String],
    name: &str,
    command: &str,
    timeout: u64,
) -> StepOutcome {
    let result = process::dev(
        runner,
        repo,
        &target.dir(repo),
        &process::split(command),
        Some(timeout),
    );
    let output = result.combined();

    if name == "format" {
        format_step(&output, changed)
    } else {
        diagnostic_step(name, command, &output, result.code, changed)
    }
}

fn syntax_result(
    contract: String,
    crate_wide_debt: bool,
    details: Vec<String>,
    problems: Vec<String>,
) -> GateResult {
    // clippy and `cargo check` report the same diagnostics; count each once.
    let problems = dedupe_problems(&problems);
    let evidence: Vec<String> = details
        .into_iter()
        .chain(problems.iter().cloned())
        .collect();
    let (status, summary) = if problems.is_empty() {
        let summary = if crate_wide_debt {
            "clean on changed files; crate-wide debt is pre-existing"
        } else {
            "clean"
        };
        (PASS, summary.to_string())
    } else {
        (
            FAIL,
            format!("{} problem(s) in changed files", problems.len()),
        )
    };

    GateResult::new("syntax", status, summary, evidence)
        .contract(contract)
        .fixes(fix_hints("syntax").iter().copied())
}
