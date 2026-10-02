use crate::process::{Command, Outcome, Runner};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command as StdCommand;

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
