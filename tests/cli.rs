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

/// The fixture's member crate with two source files, committed.
fn lib_sources(repo: &tempfile::TempDir) {
    write(
        repo.path().join("lib/src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    );
    write(
        repo.path().join("lib/src/other.rs"),
        "pub fn other() -> i32 {\n    1\n}\n",
    );
    commit_all(repo.path());
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
fn help_names_the_selection_surface_and_the_verdict_vocabulary() {
    let output = Command::new(binary())
        .arg("--help")
        .output()
        .expect("the binary runs");

    assert!(output.status.success());
    let help = stdout(&output);
    assert!(help.contains("SHIP-READY"), "{help}");
    assert!(help.contains("--package"), "{help}");
    assert!(help.contains("--lang"), "{help}");
    assert!(!help.contains("--path"), "{help}");
    assert!(!help.contains("--no-diff"), "{help}");
    assert!(
        !help.contains("--base <") && !help.contains("--base="),
        "`--base` is gone (only `--baseline-lcov` stays): {help}"
    );
}

#[test]
fn the_language_module_is_inferred_from_the_environment() {
    let repo = mini_repo();

    let output = run(repo.path(), &["--list-targets"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("workspace"));

    let bare = tempfile::tempdir().expect("temp dir");
    let output = run(bare.path(), &["--list-targets"]);
    let errors = stderr(&output);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        errors.contains("no language module recognizes this repo"),
        "{errors}"
    );
    assert!(errors.contains("--lang rust"), "{errors}");
}

#[test]
fn the_language_flag_forces_a_module_inference_cannot_pick() {
    let bare = tempfile::tempdir().expect("temp dir");

    let output = run(bare.path(), &["--lang", "rust", "--list-targets"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("workspace"));
}

#[test]
fn an_unknown_language_is_a_usage_error() {
    let output = Command::new(binary())
        .args(["--lang", "python"])
        .output()
        .expect("the binary runs");

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn the_binary_lists_the_detected_targets() {
    let repo = mini_repo();

    let output = run(repo.path(), &["--list-targets"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("frontend"));
    assert!(stdout(&output).contains("workspace"));
}

#[test]
fn the_binary_measures_the_workspace_by_default() {
    let repo = mini_repo();
    lib_sources(&repo);

    let output = run(repo.path(), &["--gate", "tests"]);
    let printed = stdout(&output);

    assert_eq!(output.status.code(), Some(0), "{printed}");
    assert!(printed.contains("workspace (./)"), "{printed}");
    assert!(printed.contains("measured 4 files (2 rust)"), "{printed}");
    assert!(printed.contains("  lib/src/lib.rs"), "{printed}");
    assert!(
        !printed.contains("frontend"),
        "a standalone crate's files belong to it, not to the workspace: {printed}"
    );
}

#[test]
fn the_binary_measures_a_named_package_whole() {
    let repo = mini_repo();
    lib_sources(&repo);

    let output = run(repo.path(), &["-p", "lib", "--gate", "tests"]);
    let printed = stdout(&output);

    assert_eq!(output.status.code(), Some(0), "{printed}");
    assert!(printed.contains("lib (lib/)"), "{printed}");
    assert!(printed.contains("measured 3 files (2 rust)"), "{printed}");
    assert!(printed.contains("  src/lib.rs"), "{printed}");
    assert!(printed.contains("  src/other.rs"), "{printed}");
}

#[test]
fn an_unknown_package_is_a_setup_error() {
    let repo = mini_repo();

    let output = run(repo.path(), &["-p", "nope"]);
    let errors = stderr(&output);

    assert_eq!(output.status.code(), Some(2), "{errors}");
    assert!(errors.contains("package `nope` not found"), "{errors}");
}

#[test]
fn the_removed_scope_flags_are_rejected() {
    let repo = mini_repo();

    let output = run(repo.path(), &["--path", "lib"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!stderr(&output).is_empty());
}

fn commit_all(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(
        root,
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
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}
