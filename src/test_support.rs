use crate::config::Config;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::process::{Command, Outcome, Runner};
use crate::session::Session;
use crate::targets::{Scope, Target};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

/// A `GateRun` for tests: the rust module, diff scope, scratch at the repo root.
pub fn gate_run<'a>(
    repo: &'a Path,
    target: &'a Target,
    config: &'a Config,
    changed: &'a [String],
) -> GateRun<'a> {
    GateRun {
        repo,
        target,
        config,
        lang: Lang::Rust,
        scope: Scope::Diff,
        changed,
        gates: &[],
        scratch: repo,
        baseline_lcov: None,
    }
}

/// The common gate-test bundle: a workspace-target run over a fixture repo,
/// against the rust baseline.
pub fn gate_run_for<'a>(
    repo: &'a MiniRepo,
    config: &'a Config,
    changed: &'a [String],
) -> GateRun<'a> {
    let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
    gate_run(&repo.root, target, config, changed)
}

/// The rust baseline loaded over a fixture repo.
pub fn config_for(repo: &MiniRepo) -> Config {
    Config::load(&repo.root, &Lang::Rust).expect("config loads")
}

/// An argv array, as a config or the CLI would spell it.
pub fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// A three-gate sample report: one PASS, one FAIL with evidence and a fix,
/// one PASS — the fixture the report tests read.
pub fn sample_results() -> Vec<crate::report::GateResult> {
    use crate::report::{GateResult, FAIL, PASS};

    vec![
        GateResult::new("syntax", PASS, "clean", Vec::<String>::new())
            .contract("`.mido.toml` [syntax] fmt/lint/typecheck"),
        GateResult::new(
            "size",
            FAIL,
            "2 functions over the ceiling",
            [
                "file src/components/task/creator.rs: 512 code lines (FAIL, fail >= 500)"
                    .to_string(),
                "function create_task (src/components/task/creator.rs): 71 sloc -> function_loc"
                    .to_string(),
            ],
        )
        .contract("`.mido.toml` [size] file_loc.fail = 500")
        .fixes(["split `create_task` along a real seam"]),
        GateResult::new("tests", PASS, "412 passed, 0 failed", Vec::<String>::new())
            .contract("`.mido.toml` [tests] command"),
    ]
}

/// A `Session` with the defaults a test does not care about.
pub fn session_for(repo: &MiniRepo, config: Config) -> Session {
    Session {
        repo: repo.root.clone(),
        config,
        lang: Lang::Rust,
        scope: Scope::Diff,
        base: "HEAD".to_string(),
        changed: Vec::new(),
        revision: "abc".to_string(),
        dirty: "def".to_string(),
        gates: crate::gate::GATES.to_vec(),
        scratch: repo.root.join("scratch"),
        baseline_lcov: None,
        report_path: None,
        apply_aid: false,
        as_json: false,
        selection: "auto".to_string(),
    }
}

/// Create an empty file at `path` under the fixture root, parents included.
pub fn touch(repo: &MiniRepo, path: &str) {
    let full = repo.root.join(path);
    std::fs::create_dir_all(full.parent().expect("parent")).expect("dir");
    std::fs::write(&full, "").expect("file");
}

/// Drive the binary's contract in-process: argv in, `(exit code, stdout, stderr)` out.
pub fn run_cli(argv: &[&str], runner: &FakeRunner) -> (i32, String, String) {
    let args = crate::cli::parse_from(argv);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = crate::cli::main_with(
        &args,
        runner,
        &mut out,
        &mut err,
        crate::style::Style::plain(),
    );
    (
        code,
        String::from_utf8_lossy(&out).to_string(),
        String::from_utf8_lossy(&err).to_string(),
    )
}

/// A runner whose git answers say "one changed file under lib/".
pub fn changed_runner(tests: (i32, &str)) -> FakeRunner {
    FakeRunner::with(&[
        ("merge-base", 0, "base\n"),
        ("--name-only", 0, "lib/src/foo.rs\n"),
        ("hash-object", 0, "dirtyhash\n"),
        ("cargo test", tests.0, tests.1),
    ])
}

/// A `Runner` that answers from a needle table instead of spawning processes.
#[derive(Default)]
pub struct FakeRunner {
    pub responses: Vec<(String, Outcome)>,
    pub tools: Vec<String>,
    pub calls: RefCell<Vec<Command>>,
}

impl FakeRunner {
    pub fn with(responses: &[(&str, i32, &str)]) -> Self {
        Self {
            responses: responses
                .iter()
                .map(|(needle, code, output)| {
                    (
                        (*needle).to_string(),
                        Outcome::new(*code, (*output).to_string(), ""),
                    )
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn tool(mut self, tool: &str) -> Self {
        self.tools.push(tool.to_string());
        self
    }

    pub fn called_with(&self, needle: &str) -> bool {
        self.calls
            .borrow()
            .iter()
            .any(|command| command_line(command).contains(needle))
    }
}

/// The needle is matched against the whole argv, joined with spaces.
fn command_line(command: &Command) -> String {
    format!(
        "{} {}",
        command.args.join(" "),
        command.stdin.clone().unwrap_or_default()
    )
}

impl Runner for FakeRunner {
    fn exec(&self, command: &Command) -> Outcome {
        self.calls.borrow_mut().push(command.clone());
        let line = command_line(command);
        for (needle, outcome) in &self.responses {
            if line.contains(needle.as_str()) {
                return outcome.clone();
            }
        }
        Outcome::new(0, "", "")
    }

    fn has(&self, tool: &str) -> bool {
        self.tools.iter().any(|known| known == tool)
    }
}

/// A miniature of the repo shape the ladder is meant to handle: one workspace
/// plus excluded crates, written into a fresh temporary directory.
pub struct MiniRepo {
    pub root: PathBuf,
    _tmp: tempfile::TempDir,
}

pub const WORKSPACE_MANIFEST: &str = "\
[workspace]
resolver = \"2\"
members = [\"cli\", \"api\", \"lib\"]
exclude = [\"frontend\", \"desktop\"]
";

impl MiniRepo {
    pub fn build(config: Option<&str>) -> Self {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join("Cargo.toml"), WORKSPACE_MANIFEST).expect("root manifest");
        for member in ["cli", "api", "lib"] {
            std::fs::create_dir_all(root.join(member)).expect("member dir");
            std::fs::write(
                root.join(member).join("Cargo.toml"),
                format!("[package]\nname = \"{member}\"\n"),
            )
            .expect("member manifest");
        }
        for crate_name in ["frontend", "desktop"] {
            std::fs::create_dir_all(root.join(crate_name)).expect("crate dir");
            std::fs::write(
                root.join(crate_name).join("Cargo.toml"),
                format!("[package]\nname = \"{crate_name}\"\n"),
            )
            .expect("crate manifest");
        }
        std::fs::create_dir_all(root.join("scripts")).expect("scripts dir");

        if let Some(config) = config {
            std::fs::write(root.join(".mido.toml"), dedent(config)).expect("config");
        }
        Self { root, _tmp: tmp }
    }

    pub fn git(&self) -> &Self {
        run_git(&self.root, &["init", "-q", "-b", "main"]);
        run_git(&self.root, &["add", "-A"]);
        run_git(
            &self.root,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "base",
            ],
        );
        self
    }
}

fn run_git(root: &std::path::Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

/// `textwrap.dedent`: drop the common indentation, keep every line — including
/// the leading blank one, so line numbers in a diagnosis match the file.
pub fn dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    let dedented: Vec<String> = lines
        .iter()
        .map(|line| {
            if !line.trim().is_empty() && line.len() >= indent {
                line[indent..].to_string()
            } else {
                line.trim_start().to_string()
            }
        })
        .collect();
    format!("{}\n", dedented.join("\n"))
}
