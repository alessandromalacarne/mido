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
use crate::metrics::Measurements;
use crate::process::Runner;
use crate::report::{gate_line, gate_progress_line, GateResult, SKIPPED};
use crate::style::Style;
use crate::targets::Target;
use std::io::Write;
use std::path::Path;

/// Everything one target's run of the selected gates needs.
pub struct GateRun<'a> {
    pub repo: &'a Path,
    pub target: &'a Target,
    pub config: &'a Config,
    pub lang: Lang,
    /// The target's own files, scoped from what git sees.
    pub files: &'a [String],
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
    let measurements = measure(runner, run);
    run_each(runner, run, &measurements, out, style)
}

/// The measured files that still exist — the counting tools choke on a path
/// that is not there, so deleted files never reach them. The files are the
/// target's own, so they resolve under the target's directory.
fn existing_sources(run: &GateRun<'_>) -> Vec<String> {
    let dir = run.target.dir(run.repo);
    run.files
        .iter()
        .filter(|path| run.lang.is_source(path) && dir.join(path).exists())
        .cloned()
        .collect()
}

/// What one measurement pass answered. The size and analysis gates are the
/// only ones that judge readings, so nothing else pays for the pass.
fn measure(runner: &dyn Runner, run: &GateRun<'_>) -> Measurements {
    let wants_measurements = run
        .gates
        .iter()
        .any(|gate| matches!(gate, Gate::Size | Gate::Analysis));
    let source_files = existing_sources(run);
    if source_files.is_empty() || !wants_measurements {
        return Measurements::default();
    }
    run.lang
        .measure(runner, run.repo, run.target, &source_files)
}

/// Every selected gate, in turn, with its line and evidence printed.
fn run_each(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    measurements: &Measurements,
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
        let result = run_one(runner, run, *gate, measurements);
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
    measurements: &Measurements,
) -> GateResult {
    let GateRun { target, config, .. } = *run;

    if !config.enabled(gate.name(), Some(&target.name)) {
        return GateResult::new(
            gate.name(),
            SKIPPED,
            "`enabled = false` in .mido.toml",
            ["waived by the config — the gate is not judged, and the verdict names it"],
        )
        .contract(format!(
            "{} [{}] enabled = false",
            config.source(),
            gate.name()
        ));
    }

    match gate {
        Gate::Syntax => syntax::gate_syntax(runner, run),
        Gate::Size => size::gate_size(run, measurements),
        Gate::Analysis => analysis::gate_analysis(run, measurements),
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
