//! Gate 6 — do the tests actually detect broken code?

use crate::config::Config;
use crate::error::GuardrailsError;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::{Lang, MutationScope, MutationSummary};
use crate::metrics::percent;
use crate::process::last_lines;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::{Scope, Target};

pub fn gate_mutation(runner: &dyn Runner, run: &GateRun<'_>) -> GateResult {
    let GateRun {
        repo,
        target,
        config,
        lang,
        scope,
        changed,
        ..
    } = *run;
    let MutationSettings {
        minimum,
        timeout,
        configured,
    } = mutation_settings(config, target);
    let mutable = mutable_sources(changed, lang);

    // A run pointed at paths has no diff to patch, so the files themselves are
    // the scope; a whole-target run mutates the package entire. Nothing mutable
    // is nothing to measure — never a pass.
    if scope != Scope::Diff && mutable.is_empty() {
        return nothing_mutable(config, lang, scope);
    }

    let args = match mutation_command(runner, run, lang, &configured, &mutable) {
        Ok(args) => args,
        Err(error) => return unpatched(error),
    };
    let contract = mutation_contract(config, minimum, &configured, &args);

    let result = process::dev(
        runner,
        lang.env_tool(),
        &target.dir(repo),
        &args,
        Some(timeout.max(0) as u64),
    );
    judge_mutation(&result, &contract, minimum, timeout, lang)
}

/// The changed files the mutation tool can mutate.
fn mutable_sources(changed: &[String], lang: Lang) -> Vec<String> {
    changed
        .iter()
        .filter(|path| lang.is_source(path))
        .cloned()
        .collect()
}

/// The `[mutation]` settings one run reads.
struct MutationSettings {
    minimum: f64,
    timeout: i64,
    configured: String,
}

fn mutation_settings(config: &Config, target: &Target) -> MutationSettings {
    MutationSettings {
        minimum: config.float("mutation", "kill_rate_min", 70.0, Some(&target.name)),
        timeout: config.int("mutation", "timeout_secs", 3600, Some(&target.name)),
        configured: config.text_setting("mutation", "scope", "changed", Some(&target.name)),
    }
}

/// The contract line the gate cites, whatever its verdict.
fn mutation_contract(config: &Config, minimum: f64, configured: &str, args: &[String]) -> String {
    format!(
        "{} [mutation] kill_rate_min={minimum}, scope={configured} via `{}`",
        config.source(),
        args.join(" ")
    )
}

/// The mutation command: the module's command plus its scope flags and the
/// per-mutant timeout.
fn mutation_command(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    lang: Lang,
    configured: &str,
    mutable: &[String],
) -> Result<Vec<String>, GuardrailsError> {
    let GateRun {
        repo,
        target,
        config,
        scope,
        scratch,
        ..
    } = *run;
    let mut args = config
        .argv("mutation", "command", target)
        .or_else(|| lang.fallback_argv("mutation", "command"))
        .unwrap_or_default();
    let patch = scratch.join(format!("guardrails-changed-{}.patch", target.name));
    let scoped = MutationScope {
        configured,
        scope,
        changed: mutable,
        patch: &patch,
    };
    args.extend(lang.mutation_scope_args(runner, repo, target, scoped)?);

    args.push("--timeout".to_string());
    args.push(lang.mutation_timeout().to_string());
    Ok(args)
}

/// The verdict for a patch that could not be written: INCOMPLETE, never a pass.
fn unpatched(error: GuardrailsError) -> GateResult {
    GateResult::new("mutation", INCOMPLETE, error.message(), [error.render()])
        .fixes(Gate::Mutation.fix_hints().iter().copied())
}

/// The verdict for a scope with nothing the mutation tool can mutate.
fn nothing_mutable(config: &Config, lang: Lang, scope: Scope) -> GateResult {
    let label = lang.source_label();
    let tool = lang.mutation_tool();
    let (place, contract) = match scope {
        Scope::Paths => ("the paths given", "explicit paths"),
        _ => ("the target", "whole target"),
    };
    GateResult::new(
        "mutation",
        INCOMPLETE,
        format!("no {label} file in {place} — nothing to mutate"),
        [format!("{tool} mutates {label} files; {place} holds none")],
    )
    .contract(format!(
        "{} [mutation] scope={contract}, no {label} file to mutate",
        config.source()
    ))
    .fixes(Gate::Mutation.fix_hints().iter().copied())
}

fn judge_mutation(
    result: &process::Outcome,
    contract: &str,
    minimum: f64,
    timeout: i64,
    lang: Lang,
) -> GateResult {
    let output = result.combined();

    if result.code == process::TIMEOUT_EXIT {
        return incomplete(
            format!("timed out after {timeout}s"),
            last_lines(&output, 10),
            contract,
        );
    }

    let Some(summary) = lang.mutation_summary(&output) else {
        return incomplete(
            format!("{} produced no summary", lang.mutation_tool()),
            last_lines(&output, 10),
            contract,
        );
    };

    let killed = summary.caught + summary.skipped;
    let rate = percent(killed, killed + summary.missed);
    let status = if rate < minimum { FAIL } else { PASS };
    GateResult::new(
        "mutation",
        status,
        format!("{rate:.1}% killed (min {minimum})"),
        mutation_details(&summary, &output),
    )
    .contract(contract)
    .fixes(Gate::Mutation.fix_hints().iter().copied())
}

/// The counts line plus every survivor the run listed. Mutants an `--iterate`
/// run skipped were caught (or unviable) in a previous run and count as killed.
fn mutation_details(summary: &MutationSummary, output: &str) -> Vec<String> {
    let killed = summary.caught + summary.skipped;
    let total = summary.total + summary.skipped;
    let rate = percent(killed, killed + summary.missed);
    let skipped_note = if summary.skipped > 0 {
        format!(
            ", {} previously caught or unviable (skipped)",
            summary.skipped
        )
    } else {
        String::new()
    };
    let mut details = vec![format!(
        "{total} mutants: {} caught, {} missed, {} unviable{skipped_note} -> {rate:.1}% killed",
        summary.caught, summary.missed, summary.unviable
    )];
    details.extend(
        output
            .lines()
            .filter(|line| line.starts_with("MISSED"))
            .map(|line| line.trim().to_string()),
    );
    details
}

/// An INCOMPLETE verdict carrying the tool's own tail as evidence.
fn incomplete(summary: String, details: Vec<String>, contract: &str) -> GateResult {
    GateResult::new("mutation", INCOMPLETE, summary, details)
        .contract(contract)
        .fixes(Gate::Mutation.fix_hints().iter().copied())
}

#[cfg(test)]
mod tests;
