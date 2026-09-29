//! The six gates, in the order they run.

pub mod analysis;
pub mod coverage;
pub mod diagnostics;
pub mod mutation;
pub mod size;
pub mod suite;
pub mod syntax;

use crate::config::Config;
use crate::metrics::Unit;
use crate::process::Runner;
use crate::report::{GateResult, SKIPPED};
use crate::targets::Target;
use std::io::Write;
use std::path::Path;

pub const RUST_EXT: &str = ".rs";

pub fn fix_hints(gate: &str) -> &'static [&'static str] {
    match gate {
        "syntax" => &[
            "fix the diagnostics in the changed files",
            "formatting alone may be auto-fixed: run the formatter in write mode, then re-run",
        ],
        "size" => &[
            "split along a real seam — moving the code into another file does not pass this gate",
        ],
        "analysis" => &[
            "extract the hard-to-hold unit; padding comments to raise MI does not pass this gate",
        ],
        "tests" => {
            &["fix the failing tests; never skip, ignore or loosen an assertion to go green"]
        }
        "coverage" => &["add behavior tests for the uncovered lines of the changed files"],
        "mutation" => {
            &["every survivor needs a real assertion, or a written equivalence justification"]
        }
        _ => &[],
    }
}

/// Render a threshold the way Python's `:g` did: `20`, not `20.0`.
pub fn general(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Everything one target's run of the selected gates needs.
pub struct GateRun<'a> {
    pub repo: &'a Path,
    pub target: &'a Target,
    pub config: &'a Config,
    pub changed: &'a [String],
    pub gates: &'a [String],
    pub scratch: &'a Path,
    pub baseline_lcov: Option<&'a Path>,
}

pub fn run_gates(runner: &dyn Runner, run: &GateRun<'_>, out: &mut dyn Write) -> Vec<GateResult> {
    let rust_changed: Vec<String> = run
        .changed
        .iter()
        .filter(|path| path.ends_with(RUST_EXT))
        .cloned()
        .collect();
    let mut tool_errors: Vec<String> = Vec::new();

    // Function metrics serve the size and analysis gates only; nothing else pays
    // for a rust-code-analysis pass.
    let wants_units = run
        .gates
        .iter()
        .any(|gate| gate == "size" || gate == "analysis");
    let units: Vec<Unit> = if !rust_changed.is_empty() && wants_units {
        analysis::analysis_units(
            runner,
            run.repo,
            run.target,
            &rust_changed,
            &mut tool_errors,
        )
    } else {
        Vec::new()
    };

    let mut results: Vec<GateResult> = Vec::new();
    for gate in run.gates {
        let result = run_one(runner, run, gate, &units, &tool_errors);
        print_result(out, &result);
        results.push(result);
    }
    results
}

fn run_one(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    gate: &str,
    units: &[Unit],
    tool_errors: &[String],
) -> GateResult {
    let GateRun {
        repo,
        target,
        config,
        changed,
        scratch,
        baseline_lcov,
        ..
    } = *run;

    if !config.enabled(gate, Some(&target.name)) {
        return GateResult::new(
            gate,
            SKIPPED,
            "`enabled = false` in .guardrails.toml",
            ["a skipped gate is not a passed gate — the waiver has to be written down"],
        )
        .contract(format!("{} [{gate}] enabled = false", config.source()));
    }

    match gate {
        "syntax" => syntax::gate_syntax(runner, repo, target, changed, config),
        "size" => size::gate_size(runner, repo, target, changed, config, units, tool_errors),
        "analysis" => analysis::gate_analysis(target, config, units, tool_errors),
        "tests" => suite::gate_tests(runner, repo, target, config),
        "coverage" => coverage::gate_coverage(
            runner,
            repo,
            target,
            config,
            changed,
            baseline_lcov,
            scratch,
        ),
        _ => mutation::gate_mutation(runner, repo, target, config, scratch),
    }
}

fn print_result(out: &mut dyn Write, result: &GateResult) {
    let _ = writeln!(
        out,
        "[{}] {}: {}",
        result.status, result.name, result.summary
    );
    for line in &result.details {
        let _ = writeln!(out, "    {line}");
    }
    let _ = writeln!(out);
}
