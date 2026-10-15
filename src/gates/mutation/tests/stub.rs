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
    /// The diff the gate's `git diff` call answers with.
    diff: String,
    /// The test's own needle answers, tried before the stub's.
    custom: Vec<(String, i32, String)>,
}

impl MutationStub {
    pub(super) fn new(passes: Vec<Option<Pass>>) -> Self {
        let mut stub = Self {
            runner: FakeRunner::default(),
            passes: RefCell::new(passes.into()),
            diff: change_diff(&["src/foo.rs"]),
            custom: Vec::new(),
        };
        stub.rebuild();
        stub
    }

    pub(super) fn answers(mut self, responses: &[(&str, i32, &str)]) -> Self {
        self.custom = responses
            .iter()
            .map(|(needle, code, output)| (needle.to_string(), *code, output.to_string()))
            .collect();
        self.rebuild();
        self
    }

    /// The change the stub says the run is scoped to: a diff touching `paths`.
    pub(super) fn change(mut self, paths: &[&str]) -> Self {
        self.diff = change_diff(paths);
        self.rebuild();
        self
    }

    /// The diff the gate reads back out of its `--in-diff` file.
    pub(super) fn diff_text(&self) -> &str {
        &self.diff
    }

    pub(super) fn called_with(&self, needle: &str) -> bool {
        self.runner.called_with(needle)
    }

    /// Whether some call's argv matches `args` exactly, argument by argument.
    pub(super) fn called_exactly(&self, args: &[&str]) -> bool {
        let expected: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        self.runner
            .calls
            .borrow()
            .iter()
            .any(|call| call.args == expected)
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

    /// The argv of every git query, in order.
    pub(super) fn git_calls(&self) -> Vec<Vec<String>> {
        self.runner
            .calls
            .borrow()
            .iter()
            .filter(|call| call.args.first().map(String::as_str) == Some("git"))
            .map(|call| call.args.clone())
            .collect()
    }

    /// Rebuild the runner: the test's answers first, then the stub's own.
    fn rebuild(&mut self) {
        let mut responses: Vec<(String, Outcome)> = self
            .custom
            .iter()
            .map(|(needle, code, output)| (needle.clone(), Outcome::new(*code, output.clone(), "")))
            .collect();
        responses.push((
            "git diff".to_string(),
            Outcome::new(0, self.diff.clone(), ""),
        ));
        self.runner = FakeRunner {
            responses,
            ..FakeRunner::default()
        };
    }
}

/// The minimal unified diff a `git diff` would print for these paths.
fn change_diff(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|path| {
            format!("diff --git a/{path} b/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
        })
        .collect()
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
