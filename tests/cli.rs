//! The binary's contract: argv in, exit code and streams out.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mido")
}

/// A miniature workspace repo: members plus excluded crates, like the real thing.
fn mini_repo() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path();
    write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"lib\"]\nexclude = [\"frontend\"]\n",
    );
    write(root.join("lib/Cargo.toml"), "[package]\nname = \"lib\"\n");
    write(
        root.join("frontend/Cargo.toml"),
        "[package]\nname = \"frontend\"\n",
    );
    directory
}

fn write(path: PathBuf, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, body).expect("file written");
}

fn run(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(binary());
    command.arg("--repo").arg(root);
    command.args(args);
    command.output().expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn help_names_the_verdict_vocabulary() {
    let output = Command::new(binary())
        .arg("--help")
        .output()
        .expect("the binary runs");

    assert!(output.status.success());
    assert!(stdout(&output).contains("SHIP-READY"));
}

#[test]
fn the_binary_lists_the_detected_targets() {
    let repo = mini_repo();

    let output = run(repo.path(), &["--list-targets"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("frontend"));
    assert!(stdout(&output).contains("workspace"));
}
