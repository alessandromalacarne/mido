//! The six gates, in the order they run.

pub mod analysis;
pub mod coverage;
pub mod mutation;
pub mod size;
pub mod suite;
pub mod syntax;

pub use crate::gate::Gate;

use crate::config::Config;
use crate::lang::Lang;
use crate::metrics::Unit;
use crate::process::Runner;
use crate::report::{gate_line, gate_progress_line, GateResult, SKIPPED};
use crate::style::Style;
use crate::targets::{Scope, Target};
use std::io::Write;
use std::path::Path;

/// Everything one target's run of the selected gates needs.
pub struct GateRun<'a> {
    pub repo: &'a Path,
    pub target: &'a Target,
    pub config: &'a Config,
    pub lang: Lang,
    pub scope: Scope,
    pub changed: &'a [String],
    pub gates: &'a [Gate],
    pub scratch: &'a Path,
    pub baseline_lcov: Option<&'a Path>,
}

pub fn run_gates(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    out: &mut dyn Write,
    style: Style,
) -> Vec<GateResult> {
    let source_changed = existing_sources(run);
    let mut tool_errors: Vec<String> = Vec::new();
    let units = measured_units(runner, run, &source_changed, &mut tool_errors);
    run_each(runner, run, &units, &tool_errors, out, style)
}

/// The changed files that still exist — the counting tools choke on a path
/// that is not there, so deleted files never reach them.
fn existing_sources(run: &GateRun<'_>) -> Vec<String> {
    run.changed
        .iter()
        .filter(|path| run.lang.is_source(path) && run.repo.join(path).exists())
        .cloned()
        .collect()
}

/// Function metrics serve the size and analysis gates only; nothing else pays
/// for a rust-code-analysis pass.
fn measured_units(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    source_changed: &[String],
    tool_errors: &mut Vec<String>,
) -> Vec<Unit> {
    let wants_units = run
        .gates
        .iter()
        .any(|gate| matches!(gate, Gate::Size | Gate::Analysis));
    if source_changed.is_empty() || !wants_units {
        return Vec::new();
    }
    run.lang
        .analysis_units(runner, run.repo, run.target, source_changed, tool_errors)
}

/// Every selected gate, in turn, with its line and evidence printed.
fn run_each(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    units: &[Unit],
    tool_errors: &[String],
    out: &mut dyn Write,
    style: Style,
) -> Vec<GateResult> {
    let total = run.gates.len();
    let width = run
        .gates
        .iter()
        .map(|gate| gate.name().len())
        .max()
        .unwrap_or(0);
    let mut results: Vec<GateResult> = Vec::new();
    for (position, gate) in run.gates.iter().enumerate() {
        announce(out, position + 1, total, width, gate.name(), style);
        let result = run_one(runner, run, *gate, units, tool_errors);
        print_result(out, position + 1, total, width, &result, style);
        results.push(result);
    }
    let _ = writeln!(out);
    results
}

fn run_one(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    gate: Gate,
    units: &[Unit],
    tool_errors: &[String],
) -> GateResult {
    let GateRun { target, config, .. } = *run;

    if !config.enabled(gate.name(), Some(&target.name)) {
        return GateResult::new(
            gate.name(),
            SKIPPED,
            "`enabled = false` in .mido.toml",
            ["a skipped gate is not a passed gate — the waiver has to be written down"],
        )
        .contract(format!(
            "{} [{}] enabled = false",
            config.source(),
            gate.name()
        ));
    }

    match gate {
        Gate::Syntax => syntax::gate_syntax(runner, run),
        Gate::Size => size::gate_size(runner, run, units, tool_errors),
        Gate::Analysis => analysis::gate_analysis(run, units, tool_errors),
        Gate::Tests => suite::gate_tests(runner, run),
        Gate::Coverage => coverage::gate_coverage(runner, run),
        Gate::Mutation => mutation::gate_mutation(runner, run),
    }
}

/// The live line a long gate runs under; only a terminal can rewrite it in
/// place, so a piped run never sees it.
fn announce(
    out: &mut dyn Write,
    position: usize,
    total: usize,
    width: usize,
    gate: &str,
    style: Style,
) {
    if !style.on() {
        return;
    }
    let _ = write!(
        out,
        "{}",
        gate_progress_line(position, total, width, gate, style)
    );
    let _ = out.flush();
}

fn print_result(
    out: &mut dyn Write,
    position: usize,
    total: usize,
    width: usize,
    result: &GateResult,
    style: Style,
) {
    if style.on() {
        let _ = write!(out, "\r\u{1b}[2K");
    }
    let _ = writeln!(out, "{}", gate_line(position, total, width, result, style));
    for line in &result.details {
        // Some gates carry their own summary in the evidence list; it is already
        // the line above, so it is not worth saying twice.
        if line != &result.summary {
            let _ = writeln!(out, "      {}", style.dim(line));
        }
    }
}

#[cfg(test)]
mod tests;
