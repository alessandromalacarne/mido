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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root).expect("config loads")
    }

    #[test]
    fn default_steps_are_format_lint_and_typecheck() {
        let repo = repo();
        let config = config_for(&repo);
        let target = Target::workspace_target();

        let steps = syntax_steps(&Table::new(), &config, &target);
        let names: Vec<&str> = steps.iter().map(|(name, _)| name.as_str()).collect();

        assert_eq!(names, vec!["format", "lint", "typecheck"]);
        assert_eq!(step_text(&steps[0].1), "cargo fmt --check");
        assert_eq!(
            step_text(&steps[1].1),
            "cargo clippy --all-targets -- -D warnings"
        );
        assert_eq!(step_text(&steps[2].1), "cargo check");
    }

    #[test]
    fn a_configured_command_runs_first() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [syntax]
            command = \"cargo build\"
        ",
        ));
        let config = config_for(&repo);
        let target = Target::workspace_target();
        let section = config.section("syntax", Some("workspace"));

        let steps = syntax_steps(&section, &config, &target);

        assert_eq!(steps[0].0, "command");
        assert_eq!(step_text(&steps[0].1), "cargo build");
    }

    #[test]
    fn an_empty_command_setting_is_not_a_step() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [syntax]
            command = \"\"
        ",
        ));
        let config = config_for(&repo);
        let target = Target::workspace_target();

        let steps = syntax_steps(
            &config.section("syntax", Some("workspace")),
            &config,
            &target,
        );

        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].0, "format");
    }

    #[test]
    fn a_non_string_command_is_reported_rather_than_run() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [syntax]
            command = 3
        ",
        ));
        let config = config_for(&repo);
        let runner = FakeRunner::default().tool("cargo");

        let result = gate_syntax(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/main.rs".to_string()],
            &config,
        );

        assert_eq!(result.status, FAIL);
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("must be a string, got int")));
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("unusable command")));
    }

    #[test]
    fn clippy_diagnostic_in_a_changed_file_fails_the_syntax_gate() {
        let repo = repo();
        let clippy = "warning: unused variable: `x`\n  --> src/components/foo.rs:12:5\nwarning: pre-existing, untouched\n  --> src/legacy/old.rs:3:1\n";
        let runner = FakeRunner::with(&[("clippy", 1, clippy), ("fmt", 0, ""), ("check", 0, "")])
            .tool("cargo");

        let result = gate_syntax(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/components/foo.rs".to_string()],
            &config_for(&repo),
        );

        assert_eq!(result.name, "syntax");
        assert_eq!(result.status, FAIL);
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("src/components/foo.rs:12:5")));
    }

    #[test]
    fn the_same_diagnostic_from_clippy_and_check_is_counted_once() {
        let repo = repo();
        let clippy = "error: unused variable: `x`\n  --> src/foo.rs:9:9\n";
        let check = "warning: unused variable: `x`\n  --> src/foo.rs:9:9\n";
        let runner =
            FakeRunner::with(&[("clippy", 1, clippy), ("fmt", 0, ""), ("check", 1, check)])
                .tool("cargo");

        let result = gate_syntax(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &config_for(&repo),
        );

        assert_eq!(result.status, FAIL);
        assert_eq!(
            result
                .details
                .iter()
                .filter(|line| line.contains("unused variable"))
                .count(),
            1
        );
    }

    #[test]
    fn crate_wide_debt_without_a_changed_file_passes_but_says_so() {
        let repo = repo();
        let clippy = "warning: pre-existing\n  --> src/legacy/old.rs:3:1\n";
        let runner = FakeRunner::with(&[("clippy", 1, clippy), ("fmt", 0, ""), ("check", 0, "")])
            .tool("cargo");

        let result = gate_syntax(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["lib/src/lib.rs".to_string()],
            &config_for(&repo),
        );

        assert_eq!(result.status, PASS);
        assert!(result.summary.contains("pre-existing"));
    }

    #[test]
    fn unattributable_linter_output_is_not_a_pass() {
        let repo = repo();
        let runner = FakeRunner::with(&[
            (
                "clippy",
                1,
                "error: something exploded in a way this runner cannot attribute\n",
            ),
            ("fmt", 0, ""),
            ("check", 0, ""),
        ])
        .tool("cargo");

        let result = gate_syntax(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &config_for(&repo),
        );

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("cannot attribute"));
    }

    #[test]
    fn the_syntax_contract_records_every_command() {
        let repo = MiniRepo::build(Some("version = 1\n"));
        let runner = FakeRunner::default().tool("cargo");

        let result = gate_syntax(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &[],
            &config_for(&repo),
        );

        assert!(result.contract.contains("`.guardrails.toml` [syntax]"));
        assert!(result.contract.contains("format='cargo fmt --check'"));
        assert!(result.contract.contains("lint='cargo clippy"));
        assert_eq!(result.fixes.len(), 2);
        assert!(result.fixes[0].contains("fix the diagnostics"));
    }
}
