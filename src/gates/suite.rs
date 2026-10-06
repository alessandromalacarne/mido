//! Gate 4 — the full suite.

use crate::config::Config;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::TestSummary;
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
    let contract = tests_contract(config, &commands);

    let mut tally = SuiteTally::default();
    for argv in &commands {
        tally.add(run_command(runner, run, argv, timeout));
    }

    let status = if tally.failed { FAIL } else { PASS };
    let summary = if tally.details.is_empty() {
        "no test command".to_string()
    } else {
        tally.details.join("; ")
    };
    let mut details = tally.details;
    details.extend(tally.problems);

    GateResult::new("tests", status, summary, details)
        .contract(contract)
        .fixes(Gate::Tests.fix_hints().iter().copied())
}

/// The contract line: every command the suite runs, in order.
fn tests_contract(config: &Config, commands: &[Vec<String>]) -> String {
    format!(
        "{} [tests] {}",
        config.source(),
        commands
            .iter()
            .map(|argv| argv.join(" "))
            .collect::<Vec<_>>()
            .join(" && ")
    )
}

struct CommandVerdict {
    details: Vec<String>,
    problems: Vec<String>,
    failed: bool,
}

/// The suite's running tally, command by command.
#[derive(Default)]
struct SuiteTally {
    details: Vec<String>,
    problems: Vec<String>,
    failed: bool,
}

impl SuiteTally {
    fn add(&mut self, verdict: CommandVerdict) {
        self.details.extend(verdict.details);
        self.problems.extend(verdict.problems);
        self.failed = self.failed || verdict.failed;
    }
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
        return no_summary_verdict(&command, result.code, &output);
    };
    summary_verdict(&command, result.ok(), &summary)
}

/// The verdict for a command that printed no `test result:` line.
fn no_summary_verdict(command: &str, code: i32, output: &str) -> CommandVerdict {
    CommandVerdict {
        details: process::last_lines_with(output, 10, "  "),
        problems: vec![format!("`{command}` printed no test summary (exit {code})")],
        failed: true,
    }
}

/// The verdict for a command that reported a summary; a non-zero exit is a
/// failure even when every test passed.
fn summary_verdict(command: &str, ok: bool, summary: &TestSummary) -> CommandVerdict {
    let broken = summary.failed > 0 || !ok;
    let problems = if broken {
        vec![format!(
            "`{command}`: {} failed{}",
            summary.failed,
            failed_names_suffix(&summary.failed_names)
        )]
    } else {
        Vec::new()
    };

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

#[cfg(test)]
mod tests;
