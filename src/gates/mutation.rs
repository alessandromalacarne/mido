//! Gate 6 — do the tests actually detect broken code?

mod change;
mod judgment;

use crate::config::Config;
use crate::error::GuardrailsError;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::process::Runner;
use crate::report::{GateResult, INCOMPLETE, PASS};
use crate::targets::Target;
use change::ChangeScope;
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

    let change = match change_of(runner, run, lang) {
        Ok(change) => change,
        Err(error) => return unpatched(error),
    };
    let passes = match mutation_passes(run, lang, &output, change.as_ref()) {
        Ok(passes) => passes,
        Err(error) => return unpatched(error),
    };
    let contract = mutation_contract(config, minimum, &passes);

    // A change that touches no source file of the target has nothing a mutant
    // could break: say so, instead of sweeping for mutants that cannot exist.
    if let Some(change) = change.as_ref().filter(|change| !change.has_source) {
        return nothing_in_change(config, lang, &change.diff);
    }

    match judgment::mutation_reports(runner, run, &passes, timeout, &output) {
        Ok(reports) => judgment::judge_mutation(&reports, &contract, minimum),
        Err(unjudged) => judgment::incomplete(unjudged.summary, unjudged.details, &contract),
    }
}

/// The change the sweep is scoped to, unless the config asks for the whole
/// target: mutation is the slow gate, and a sweep costs what it measures.
fn change_of(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    lang: Lang,
) -> Result<Option<ChangeScope>, GuardrailsError> {
    let asked = run
        .config
        .flag("mutation", "in_diff", true, Some(&run.target.name));
    if !asked {
        return Ok(None);
    }
    change::change_scope(runner, run, lang).map(Some)
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
/// each timed, scoped to the change (when there is one) and given the run's own
/// output directory. Every pass but the first carries `--iterate`, so the
/// mutants an earlier pass caught come back as skipped — and skipped counts as
/// killed.
fn mutation_passes(
    run: &GateRun<'_>,
    lang: Lang,
    output: &Path,
    change: Option<&ChangeScope>,
) -> Result<Vec<MutationPass>, GuardrailsError> {
    let base = mutation_base(run, lang, change)?;
    Ok(passes_over(&base, run, lang, output, change))
}

/// The configured mutation command (or the module's fallback), checked so mido
/// stays in charge of the run's state and scope.
fn mutation_base(
    run: &GateRun<'_>,
    lang: Lang,
    change: Option<&ChangeScope>,
) -> Result<Vec<String>, GuardrailsError> {
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
    if change.is_some() && names_own_diff(&base) {
        return Err(GuardrailsError::setup(
            "the configured mutation command names its own --in-diff",
        )
        .detail(
            "mido scopes a default mutation run to the change: it generates the \
             diff itself, so the two scopes would fight",
        )
        .hint(
            "drop --in-diff from [mutation] command, or set `in_diff = false` in \
             [mutation] to drive your own scope",
        ));
    }
    Ok(base)
}

/// One pass per declared test command, carrying the arguments that command
/// names — deduplicated by those arguments, so a suite never runs twice.
fn passes_over(
    base: &[String],
    run: &GateRun<'_>,
    lang: Lang,
    output: &Path,
    change: Option<&ChangeScope>,
) -> Vec<MutationPass> {
    let mut driven: Vec<Vec<String>> = Vec::new();
    let mut passes: Vec<MutationPass> = Vec::new();

    for command in lang.test_commands(run.config, run.target, run.repo) {
        let test_args = lang.mutation_test_args(&command).unwrap_or_default();
        if driven.contains(&test_args) {
            continue;
        }

        let iterate = !passes.is_empty();
        let args = pass_args(base, &test_args, iterate, output, change, lang);
        passes.push(MutationPass { command, args });
        driven.push(test_args);
    }

    if passes.is_empty() {
        let args = pass_args(base, &[], false, output, change, lang);
        passes.push(MutationPass {
            command: Vec::new(),
            args,
        });
    }

    passes
}

/// Whether the configured command names its own output directory — in any of
/// the forms cargo-mutants' parser accepts.
fn names_own_output(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg.starts_with("-o") || arg.starts_with("--output"))
}

/// Whether the configured command names its own diff scope — cargo-mutants'
/// `--in-diff` and its `-D` short form.
fn names_own_diff(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg.starts_with("-D") || arg.starts_with("--in-diff"))
}

/// One pass's argv: the configured command, timed, reading the change diff
/// when there is one, carrying the run's output directory and the test
/// arguments of the suite it drives.
fn pass_args(
    base: &[String],
    test_args: &[String],
    iterate: bool,
    output: &Path,
    change: Option<&ChangeScope>,
    lang: Lang,
) -> Vec<String> {
    let mut args = base.to_vec();
    if iterate && !args.iter().any(|arg| arg == "--iterate") {
        args.push("--iterate".to_string());
    }
    args.push("--output".to_string());
    args.push(output.to_string_lossy().to_string());
    if let Some(change) = change {
        args.push("--in-diff".to_string());
        args.push(change.diff.to_string_lossy().to_string());
    }
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

/// The verdict for a change that touches no source file of the target: there
/// is no code a mutant could break, and the gate says so instead of sweeping.
fn nothing_in_change(config: &Config, lang: Lang, diff: &Path) -> GateResult {
    let label = lang.source_label();
    GateResult::new(
        "mutation",
        PASS,
        format!("no {label} file in the change — nothing to mutate"),
        [format!("change diff: {}", diff.display())],
    )
    .contract(format!(
        "{} [mutation] no {label} file in the change",
        config.source()
    ))
}

#[cfg(test)]
mod tests;
