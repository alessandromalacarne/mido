//! Gate 4 — the full suite.

use crate::gates::{fix_hints, GateRun};
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, PASS};

pub fn gate_tests(runner: &dyn Runner, run: &GateRun<'_>) -> GateResult {
    let GateRun {
        repo,
        target,
        config,
        lang,
        ..
    } = *run;
    let timeout = config
        .int("tests", "timeout_secs", 900, Some(&target.name))
        .max(0) as u64;
    let commands = lang.test_commands(config, target, repo);
    let contract = format!(
        "{} [tests] {}",
        config.source(),
        commands
            .iter()
            .map(|argv| argv.join(" "))
            .collect::<Vec<_>>()
            .join(" && ")
    );

    let mut details: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut status = PASS;

    for argv in &commands {
        let verdict = run_command(runner, run, argv, timeout);
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
    run: &GateRun<'_>,
    argv: &[String],
    timeout: u64,
) -> CommandVerdict {
    let GateRun {
        repo, target, lang, ..
    } = *run;
    let command = argv.join(" ");
    let result = process::dev(
        runner,
        lang.env_tool(),
        &target.dir(repo),
        argv,
        Some(timeout),
    );
    let output = result.combined();

    let Some(summary) = lang.test_summary(&output) else {
        return CommandVerdict {
            details: tail_details(&output),
            problems: vec![format!(
                "`{command}` printed no test summary (exit {})",
                result.code
            )],
            failed: true,
        };
    };

    let broken = summary.failed > 0 || !result.ok();
    let mut problems = Vec::new();
    if broken {
        problems.push(format!(
            "`{command}`: {} failed{}",
            summary.failed,
            failed_names_suffix(&summary.failed_names)
        ));
    }

    CommandVerdict {
        details: vec![format!(
            "{command}: {} passed, {} failed",
            summary.passed, summary.failed
        )],
        problems,
        failed: broken,
    }
}

fn failed_names_suffix(names: &[String]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::lang::Lang;
    use crate::targets::Target;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo(config: Option<&str>) -> MiniRepo {
        MiniRepo::build(config)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root, &Lang::Rust).expect("config loads")
    }

    fn gate_run_for<'a>(repo: &'a MiniRepo, config: &'a Config) -> GateRun<'a> {
        let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
        crate::test_support::gate_run(&repo.root, target, config, &[])
    }

    #[test]
    fn failing_tests_are_named_in_the_tests_gate() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\", \"--all-features\"]
        ",
        ));
        let output = "test thing::works ... ok\ntest thing::breaks ... FAILED\n\ntest result: FAILED. 1 passed; 1 failed\n";
        let runner = FakeRunner::with(&[("cargo test", 101, output)]);

        let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo)));

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("thing::breaks"));
        assert!(result.contract.contains("cargo test --all-features"));
    }

    #[test]
    fn a_green_suite_reports_its_counts() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let output = "test result: ok. 41 passed; 0 failed; 0 ignored\n";
        let runner = FakeRunner::with(&[("cargo test", 0, output)]);

        let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo)));

        assert_eq!(result.status, PASS);
        assert_eq!(result.summary, "cargo test: 41 passed, 0 failed");
    }

    #[test]
    fn tests_gate_fails_when_the_command_prints_no_summary() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let runner = FakeRunner::with(&[("cargo test", 127, "cargo: command not found")]);

        let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo)));

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("no test summary"));
    }

    #[test]
    fn a_nonzero_exit_with_every_test_green_is_still_a_failure() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let runner =
            FakeRunner::with(&[("cargo test", 101, "test result: ok. 3 passed; 0 failed\n")]);

        let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo)));

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("`cargo test`: 0 failed"));
    }

    #[test]
    fn a_failing_suite_that_exits_zero_is_still_a_failure() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let runner =
            FakeRunner::with(&[("cargo test", 0, "test result: FAILED. 0 passed; 1 failed\n")]);

        let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo)));

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("`cargo test`: 1 failed"));
    }
}
