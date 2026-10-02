//! Gate 1 — formatter, linter, type checker.

use crate::config::Config;
use crate::gates::diagnostics::{dedupe_problems, diagnostic_step, format_step, StepOutcome};
use crate::gates::fix_hints;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, PASS};
use crate::targets::Target;
use std::path::Path;

/// The steps this target runs: an explicit `[syntax] command` first, then
/// format, lint and typecheck.
pub fn syntax_steps(config: &Config, target: &Target) -> Vec<(String, Vec<String>)> {
    let mut steps = Vec::new();

    if let Some(configured) = config.argv("syntax", "command", target) {
        steps.push(("command".to_string(), configured));
    }

    let defaults: [(&str, &str, &[&str]); 3] = [
        ("format", "format", &["cargo", "fmt", "--check"]),
        (
            "lint",
            "lint",
            &["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        ),
        ("typecheck", "typecheck", &["cargo", "check"]),
    ];
    for (name, key, default) in defaults {
        let argv = config
            .argv("syntax", key, target)
            .unwrap_or_else(|| default.iter().map(|arg| (*arg).to_string()).collect());
        steps.push((name.to_string(), argv));
    }
    steps
}

pub fn gate_syntax(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    changed: &[String],
    config: &Config,
) -> GateResult {
    let steps = syntax_steps(config, target);
    let contract = format!(
        "{} [syntax] {}",
        config.source(),
        steps
            .iter()
            .map(|(name, argv)| format!("{name}=`{}`", argv.join(" ")))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let timeout = config
        .int("syntax", "timeout_secs", 1800, Some(&target.name))
        .max(0) as u64;

    let mut details: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut crate_wide_debt = false;

    for (name, argv) in &steps {
        let outcome = run_step(runner, repo, target, changed, name, argv, timeout);
        details.extend(outcome.details);
        problems.extend(outcome.problems);
        crate_wide_debt = crate_wide_debt || outcome.crate_wide_debt;
    }

    syntax_result(contract, crate_wide_debt, details, problems)
}

fn run_step(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    changed: &[String],
    name: &str,
    argv: &[String],
    timeout: u64,
) -> StepOutcome {
    let result = process::dev(runner, &target.dir(repo), argv, Some(timeout));
    let output = result.combined();

    if name == "format" {
        format_step(&output, changed)
    } else {
        diagnostic_step(name, &argv.join(" "), &output, result.code, changed)
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

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn default_steps_are_format_lint_and_typecheck() {
        let repo = repo();
        let config = config_for(&repo);
        let target = Target::workspace_target();

        let steps = syntax_steps(&config, &target);
        let names: Vec<&str> = steps.iter().map(|(name, _)| name.as_str()).collect();

        assert_eq!(names, vec!["format", "lint", "typecheck"]);
        assert_eq!(steps[0].1, argv(&["cargo", "fmt", "--check"]));
        assert_eq!(
            steps[1].1,
            argv(&["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
        );
        assert_eq!(steps[2].1, argv(&["cargo", "check"]));
    }

    #[test]
    fn a_configured_command_runs_first() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [syntax]
            command = [\"cargo\", \"build\"]
        ",
        ));
        let config = config_for(&repo);
        let target = Target::workspace_target();

        let steps = syntax_steps(&config, &target);

        assert_eq!(steps[0].0, "command");
        assert_eq!(steps[0].1, argv(&["cargo", "build"]));
        assert_eq!(steps.len(), 4);
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

        assert!(result.contract.contains("`.mido.toml` [syntax]"));
        assert!(result.contract.contains("format=`cargo fmt --check`"));
        assert!(result.contract.contains("lint=`cargo clippy"));
        assert_eq!(result.fixes.len(), 2);
        assert!(result.fixes[0].contains("fix the diagnostics"));
    }
}
