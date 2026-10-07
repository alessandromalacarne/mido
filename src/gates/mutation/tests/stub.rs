//! The mutation gate's test double: a runner that plays cargo-mutants, writing
//! the report files each pass would leave in the gate's `--output` directory.

use crate::process::{Command, Outcome, Runner};
use crate::test_support::FakeRunner;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// One cargo-mutants pass as the tool writes it: the counts, the named mutants,
/// and the exclusion list an `--iterate` pass read at its start.
#[derive(Clone)]
pub(super) struct Pass {
    pub(super) total: i64,
    pub(super) caught: i64,
    pub(super) missed: i64,
    pub(super) unviable: i64,
    pub(super) timeout: i64,
    pub(super) skipped: Vec<&'static str>,
    pub(super) survivors: Vec<&'static str>,
    pub(super) timed_out: Vec<&'static str>,
    pub(super) baseline_failure: Option<&'static str>,
    /// `false` is a run that died before writing `end_time`.
    pub(super) finished: bool,
}

impl Default for Pass {
    fn default() -> Self {
        Self {
            total: 0,
            caught: 0,
            missed: 0,
            unviable: 0,
            timeout: 0,
            skipped: Vec::new(),
            survivors: Vec::new(),
            timed_out: Vec::new(),
            baseline_failure: None,
            finished: true,
        }
    }
}

/// A runner that plays cargo-mutants: git and cargo answers from a needle
/// table, and — as each pass runs — the report files that pass leaves in the
/// `--output` directory the gate named.
pub(super) struct MutationStub {
    runner: FakeRunner,
    passes: RefCell<VecDeque<Option<Pass>>>,
}

impl MutationStub {
    pub(super) fn new(passes: Vec<Option<Pass>>) -> Self {
        Self {
            runner: FakeRunner::default(),
            passes: RefCell::new(passes.into()),
        }
    }

    pub(super) fn answers(mut self, responses: &[(&str, i32, &str)]) -> Self {
        self.runner = FakeRunner::with(responses);
        self
    }

    pub(super) fn called_with(&self, needle: &str) -> bool {
        self.runner.called_with(needle)
    }

    /// The argv of every cargo-mutants pass, in order.
    pub(super) fn mutant_calls(&self) -> Vec<Vec<String>> {
        self.runner
            .calls
            .borrow()
            .iter()
            .filter(|call| call.args.iter().any(|arg| arg == "mutants"))
            .map(|call| call.args.clone())
            .collect()
    }
}

impl Runner for MutationStub {
    fn exec(&self, command: &Command) -> Outcome {
        if command.args.iter().any(|arg| arg == "mutants") {
            if let Some(pass) = self.passes.borrow_mut().pop_front().flatten() {
                write_pass(&output_dir(command), &pass);
            }
        }
        self.runner.exec(command)
    }

    fn has(&self, tool: &str) -> bool {
        self.runner.has(tool)
    }
}

/// The `--output` directory the pass under test was given.
fn output_dir(command: &Command) -> PathBuf {
    let args = &command.args;
    let index = args
        .iter()
        .position(|arg| arg == "--output")
        .expect("every pass names its output directory");
    PathBuf::from(&args[index + 1])
}

/// Leave the files a real pass would under `mutants.out/`.
pub(super) fn write_pass(dir: &Path, pass: &Pass) {
    let out = dir.join("mutants.out");
    std::fs::create_dir_all(&out).expect("mutants.out");
    std::fs::write(out.join("outcomes.json"), outcomes_json(pass)).expect("outcomes.json");
    if !pass.skipped.is_empty() {
        std::fs::write(
            out.join("previously_caught.txt"),
            format!("{}\n", pass.skipped.join("\n")),
        )
        .expect("previously_caught.txt");
    }
}

/// A run's `outcomes.json`, in the shape cargo-mutants writes.
fn outcomes_json(pass: &Pass) -> String {
    let mut outcomes = vec![serde_json::json!({
        "scenario": "Baseline",
        "summary": pass.baseline_failure.unwrap_or("Success"),
    })];
    for (summary, names) in [
        ("MissedMutant", &pass.survivors),
        ("Timeout", &pass.timed_out),
    ] {
        for name in names {
            outcomes.push(serde_json::json!({
                "scenario": { "Mutant": { "name": name } },
                "summary": summary,
            }));
        }
    }
    serde_json::json!({
        "outcomes": outcomes,
        "total_mutants": pass.total,
        "caught": pass.caught,
        "missed": pass.missed,
        "unviable": pass.unviable,
        "timeout": pass.timeout,
        "end_time": pass.finished.then_some("2026-10-07T04:15:17.119958003Z"),
    })
    .to_string()
}
