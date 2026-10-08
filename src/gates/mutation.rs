//! Gate 6 — do the tests actually detect broken code?

mod judgment;

use crate::config::Config;
use crate::error::GuardrailsError;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::process::Runner;
use crate::report::{GateResult, INCOMPLETE};
use crate::targets::Target;
use std::path::Path;

pub fn gate_mutation(runner: &dyn Runner, run: &GateRun<'_>) -> GateResult {
    let GateRun {
        target,
        config,
        lang,
        files,
        ..
    } = *run;
    let MutationSettings { minimum, timeout } = mutation_settings(config, target);

    // Nothing mutable is nothing to measure — never a pass.
    if !files.iter().any(|path| lang.is_source(path)) {
        return nothing_mutable(config, lang);
    }

    // The run owns its cargo-mutants state: a fresh output directory, so a
    // verdict can only ever come from the mutants this run tested.
    let output = run
        .scratch
        .join(format!("guardrails-mutants-{}", target.name));
    let _ = std::fs::remove_dir_all(&output);

    let passes = match mutation_passes(run, lang, &output) {
        Ok(passes) => passes,
        Err(error) => return unpatched(error),
    };
    let contract = mutation_contract(config, minimum, &passes);

    match judgment::mutation_reports(runner, run, &passes, timeout, &output) {
        Ok(reports) => judgment::judge_mutation(&reports, &contract, minimum),
        Err(unjudged) => judgment::incomplete(unjudged.summary, unjudged.details, &contract),
    }
}

/// The `[mutation]` settings one run reads.
struct MutationSettings {
    minimum: f64,
    timeout: i64,
}

fn mutation_settings(config: &Config, target: &Target) -> MutationSettings {
    MutationSettings {
        minimum: config.float("mutation", "kill_rate_min", 70.0, Some(&target.name)),
        timeout: config.int("mutation", "timeout_secs", 3600, Some(&target.name)),
    }
}

/// The contract line the gate cites, whatever its verdict: every pass it made.
fn mutation_contract(config: &Config, minimum: f64, passes: &[MutationPass]) -> String {
    let commands = passes
        .iter()
        .map(|pass| pass.args.join(" "))
        .collect::<Vec<_>>()
        .join(" ; ");
    format!(
        "{} [mutation] kill_rate_min={minimum} via `{commands}`",
        config.source()
    )
}

/// One mutation pass: the cargo-mutants argv, and the test command it puts in
/// front of the mutants.
struct MutationPass {
    command: Vec<String>,
    args: Vec<String>,
}

/// The passes a mutation run makes: one per test command the target declares,
/// each timed and given the run's own output directory. Every pass but the
/// first carries `--iterate`, so the mutants an earlier pass caught come back
/// as skipped — and skipped counts as killed. The package itself is the scope:
/// no diff, no patch, no file list.
fn mutation_passes(
    run: &GateRun<'_>,
    lang: Lang,
    output: &Path,
) -> Result<Vec<MutationPass>, GuardrailsError> {
    let base = run
        .config
        .argv("mutation", "command", run.target)
        .or_else(|| lang.fallback_argv("mutation", "command"))
        .unwrap_or_default();
    if names_own_output(&base) {
        return Err(GuardrailsError::setup(
            "the configured mutation command names its own --output",
        )
        .detail(
            "mido owns the mutation state: a verdict may only be decided by the \
             mutants a run itself started",
        )
        .hint(
            "drop --output from [mutation] command — mido writes into its own scratch directory",
        ));
    }

    let mut driven: Vec<Vec<String>> = Vec::new();
    let mut passes: Vec<MutationPass> = Vec::new();

    for command in lang.test_commands(run.config, run.target, run.repo) {
        let test_args = lang.mutation_test_args(&command).unwrap_or_default();
        if driven.contains(&test_args) {
            continue;
        }

        let iterate = !passes.is_empty();
        let args = pass_args(&base, &test_args, iterate, output, lang);
        passes.push(MutationPass { command, args });
        driven.push(test_args);
    }

    if passes.is_empty() {
        let args = pass_args(&base, &[], false, output, lang);
        passes.push(MutationPass {
            command: Vec::new(),
            args,
        });
    }

    Ok(passes)
}

/// Whether the configured command names its own output directory — in any of
/// the forms cargo-mutants' parser accepts.
fn names_own_output(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg.starts_with("-o") || arg.starts_with("--output"))
}

/// One pass's argv: the configured command, timed, carrying the run's output
/// directory and the test arguments of the suite it drives.
fn pass_args(
    base: &[String],
    test_args: &[String],
    iterate: bool,
    output: &Path,
    lang: Lang,
) -> Vec<String> {
    let mut args = base.to_vec();
    if iterate && !args.iter().any(|arg| arg == "--iterate") {
        args.push("--iterate".to_string());
    }
    args.push("--output".to_string());
    args.push(output.to_string_lossy().to_string());
    args.push("--timeout".to_string());
    args.push(lang.mutation_timeout().to_string());
    if !test_args.is_empty() {
        args.push("--".to_string());
        args.extend(test_args.iter().cloned());
    }
    args
}

/// The verdict for a command mido cannot drive: INCOMPLETE, never a pass.
fn unpatched(error: GuardrailsError) -> GateResult {
    GateResult::new("mutation", INCOMPLETE, error.message(), [error.render()])
        .fixes(Gate::Mutation.fix_hints().iter().copied())
}

/// The verdict for a target with nothing the mutation tool can mutate.
fn nothing_mutable(config: &Config, lang: Lang) -> GateResult {
    let label = lang.source_label();
    let tool = lang.mutation_tool();
    GateResult::new(
        "mutation",
        INCOMPLETE,
        format!("no {label} file in the target — nothing to mutate"),
        [format!(
            "{tool} mutates {label} files; this target holds none"
        )],
    )
    .contract(format!(
        "{} [mutation] no {label} file to mutate",
        config.source()
    ))
    .fixes(Gate::Mutation.fix_hints().iter().copied())
}

#[cfg(test)]
mod tests;
