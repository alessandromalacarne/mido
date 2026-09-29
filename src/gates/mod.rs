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
    // A deleted file has nothing to measure — the counting tools choke on a
    // path that is not there, so only files that still exist reach them.
    let rust_changed: Vec<String> = run
        .changed
        .iter()
        .filter(|path| path.ends_with(RUST_EXT) && run.repo.join(path).exists())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    #[test]
    fn every_gate_has_a_fix_hint() {
        for gate in [
            "syntax", "size", "analysis", "tests", "coverage", "mutation",
        ] {
            assert!(!fix_hints(gate).is_empty(), "{gate} needs a fix hint");
        }
        assert!(fix_hints("tests")[0].contains("never skip"));
        assert!(fix_hints("coverage")[0].contains("behavior tests"));
        assert!(fix_hints("mutation")[0].contains("equivalence justification"));
        assert!(fix_hints("analysis")[0].contains("padding comments"));
        assert!(fix_hints("size")[0].contains("real seam"));
        assert!(fix_hints("syntax")[0].contains("fix the diagnostics"));
        assert!(fix_hints("unknown-gate").is_empty());
    }

    #[test]
    fn a_disabled_gate_reports_itself_as_skipped() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [mutation]
            enabled = false
        ",
        ));
        let config = Config::load(&repo.root).expect("config loads");
        let mut out = Vec::new();

        let results = run_gates(
            &FakeRunner::default(),
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &[],
                gates: &["mutation".to_string()],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut out,
        );

        assert_eq!(results[0].status, SKIPPED);
        assert!(results[0].contract.contains("enabled = false"));
    }

    #[test]
    fn every_gate_reports_a_line_of_its_own() {
        let repo = repo();
        let config = Config::load(&repo.root).expect("config loads");
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 3 } }] } });
        let runner = FakeRunner::with(&[
            ("tokei", 0, &tokei.to_string()),
            ("cargo test", 0, "test result: ok. 1 passed; 0 failed\n"),
        ])
        .tool("cargo");
        let mut out = Vec::new();

        let results = run_gates(
            &runner,
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &["src/foo.rs".to_string()],
                gates: &[
                    "syntax".to_string(),
                    "size".to_string(),
                    "analysis".to_string(),
                    "tests".to_string(),
                ],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut out,
        );

        assert_eq!(results.len(), 4);
        let printed = String::from_utf8(out).expect("utf8");
        assert!(printed.contains("[PASS] syntax"));
        assert!(printed.contains("[INCOMPLETE] analysis"));
    }

    #[test]
    fn the_size_gate_gets_the_function_metrics_it_judges() {
        let repo = repo();
        std::fs::create_dir_all(repo.root.join("src")).expect("dir");
        std::fs::write(repo.root.join("src/foo.rs"), "").expect("file");
        let config = Config::load(&repo.root).expect("config loads");
        let document = serde_json::json!({
            "spaces": [{ "name": "f", "kind": "function", "metrics": { "loc": { "sloc": 5 } } }]
        });
        let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 10 } }] } });
        let runner = FakeRunner::with(&[
            ("rust-code-analysis-cli", 0, &document.to_string()),
            ("tokei", 0, &tokei.to_string()),
        ])
        .tool("cargo");

        let results = run_gates(
            &runner,
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &["src/foo.rs".to_string()],
                gates: &["size".to_string()],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut Vec::new(),
        );

        assert_eq!(results[0].name, "size");
        assert_eq!(results[0].status, crate::report::PASS);
        assert!(results[0].summary.contains("worst function 5 sloc"));
    }

    #[test]
    fn the_coverage_gate_is_dispatched_under_its_own_name() {
        let repo = repo();
        let config = Config::load(&repo.root).expect("config loads");
        let runner = FakeRunner::with(&[("llvm-cov", 101, "no coverage tool here")]).tool("cargo");

        let results = run_gates(
            &runner,
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &["src/foo.rs".to_string()],
                gates: &["coverage".to_string()],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut Vec::new(),
        );

        assert_eq!(results[0].name, "coverage");
        assert_eq!(results[0].status, crate::report::INCOMPLETE);
    }

    #[test]
    fn the_gates_run_in_the_order_they_were_asked_for() {
        let repo = repo();
        let config = Config::load(&repo.root).expect("config loads");
        let runner =
            FakeRunner::with(&[("cargo test", 0, "test result: ok. 1 passed; 0 failed\n")])
                .tool("cargo");

        let results = run_gates(
            &runner,
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &["src/foo.rs".to_string()],
                gates: &["tests".to_string(), "syntax".to_string()],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut Vec::new(),
        );

        let names: Vec<&str> = results.iter().map(|result| result.name.as_str()).collect();
        assert_eq!(names, vec!["tests", "syntax"]);
    }

    #[test]
    fn a_deleted_rust_file_is_not_measured() {
        let repo = repo();
        let config = Config::load(&repo.root).expect("config loads");
        let runner = FakeRunner::default().tool("cargo");

        let results = run_gates(
            &runner,
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &["src/gone.rs".to_string()],
                gates: &["size".to_string()],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut Vec::new(),
        );

        assert_eq!(results[0].status, crate::report::PASS);
        assert_eq!(results[0].summary, "no rust changes");
        assert!(!runner.called_with("tokei"));
    }

    #[test]
    fn units_are_only_measured_for_the_gates_that_need_them() {
        let repo = repo();
        let config = Config::load(&repo.root).expect("config loads");
        let runner = FakeRunner::with(&[("rust-code-analysis-cli", 1, "")]).tool("cargo");

        run_gates(
            &runner,
            &GateRun {
                repo: &repo.root,
                target: &Target::workspace_target(),
                config: &config,
                changed: &["src/foo.rs".to_string()],
                gates: &["syntax".to_string()],
                scratch: &repo.root,
                baseline_lcov: None,
            },
            &mut Vec::new(),
        );

        assert!(!runner.called_with("rust-code-analysis-cli"));
    }
}
