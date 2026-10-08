//! Gate 1 — formatter, linter, type checker.

use crate::config::Config;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::{Lang, StepOutcome};
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, PASS};
use crate::targets::Target;

/// The steps this target runs: an explicit `[syntax] command` first, then
/// format, lint and typecheck.
pub fn syntax_steps(config: &Config, target: &Target, lang: Lang) -> Vec<(String, Vec<String>)> {
    let mut steps = Vec::new();

    if let Some(configured) = config.argv("syntax", "command", target) {
        steps.push(("command".to_string(), configured));
    }

    for (name, key) in [
        ("format", "format"),
        ("lint", "lint"),
        ("typecheck", "typecheck"),
    ] {
        let argv = config
            .argv("syntax", key, target)
            .or_else(|| lang.fallback_argv("syntax", key));
        if let Some(argv) = argv {
            steps.push((name.to_string(), argv));
        }
    }
    steps
}

pub fn gate_syntax(runner: &dyn Runner, run: &GateRun<'_>) -> GateResult {
    let GateRun {
        target,
        config,
        lang,
        ..
    } = *run;
    let steps = syntax_steps(config, target, lang);
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

    for (name, argv) in &steps {
        let outcome = run_step(runner, run, name, argv, timeout);
        details.extend(outcome.details);
        problems.extend(outcome.problems);
    }

    syntax_result(contract, details, problems, lang)
}

fn run_step(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    name: &str,
    argv: &[String],
    timeout: u64,
) -> StepOutcome {
    let GateRun {
        repo, target, lang, ..
    } = *run;
    let result = process::dev(
        runner,
        lang.env_tool(),
        &target.dir(repo),
        argv,
        Some(timeout),
    );
    let output = result.combined();

    lang.syntax_outcome(name, &argv.join(" "), &output, result.code)
}

fn syntax_result(
    contract: String,
    details: Vec<String>,
    problems: Vec<String>,
    lang: Lang,
) -> GateResult {
    // clippy and `cargo check` report the same diagnostics; count each once.
    let problems = lang.dedupe_problems(&problems);
    let evidence: Vec<String> = details
        .into_iter()
        .chain(problems.iter().cloned())
        .collect();
    let (status, summary) = if problems.is_empty() {
        (PASS, "clean".to_string())
    } else {
        (FAIL, format!("{} problem(s)", problems.len()))
    };

    GateResult::new("syntax", status, summary, evidence)
        .contract(contract)
        .fixes(Gate::Syntax.fix_hints().iter().copied())
}

#[cfg(test)]
mod tests;
