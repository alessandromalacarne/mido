//! The binary's contract: argv in, exit code and streams out.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mido")
}

/// A miniature workspace repo: members plus excluded crates, like the real thing.
fn mini_repo(config: Option<&str>) -> tempfile::TempDir {
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
    if let Some(config) = config {
        write(root.join(".guardrails.toml"), config);
    }
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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
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
fn list_targets_prints_the_detected_set() {
    let repo = mini_repo(None);

    let output = run(repo.path(), &["--list-targets"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("frontend"));
    assert!(stdout(&output).contains("workspace"));
}

#[test]
fn an_unknown_target_exits_2_and_names_the_known_ones() {
    let repo = mini_repo(Some("version = 1\n"));

    let output = run(repo.path(), &["nope"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown target `nope`"));
    assert!(stderr(&output).contains("frontend"));
}

#[test]
fn an_invalid_config_exits_2_with_the_offending_line() {
    let repo = mini_repo(Some("version = 1\n\n[analysis]\nmin_mi = 20\n"));

    let output = run(repo.path(), &["frontend"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("line 4: unknown key `min_mi`"));
    assert!(stderr(&output).contains("did you mean `mi_min`?"));
}

#[test]
fn a_gate_name_outside_the_ladder_exits_2() {
    let repo = mini_repo(None);

    let output = run(repo.path(), &["--gate", "lint"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("possible values"));
}

#[test]
fn a_clean_tree_exits_2_rather_than_claiming_a_pass() {
    let repo = mini_repo(Some("version = 1\n"));

    let output = run(repo.path(), &["workspace"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).to_lowercase().contains("nothing"));
}
