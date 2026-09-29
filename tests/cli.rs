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
    assert!(stdout(&output).contains("--path"));
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
fn the_binary_measures_explicit_paths_without_a_diff() {
    let repo = mini_repo();
    write(
        repo.path().join("lib/src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    );
    commit_all(repo.path());
    // A change the diff would report, and the run must not.
    write(
        repo.path().join("lib/src/other.rs"),
        "pub fn other() -> i32 {\n    1\n}\n",
    );

    let output = run(
        repo.path(),
        &["--path", "lib/src/lib.rs", "--gate", "tests"],
    );
    let printed = stdout(&output);

    assert_eq!(output.status.code(), Some(0), "{printed}");
    assert!(printed.contains("scope    explicit paths"), "{printed}");
    assert!(printed.contains("  src/lib.rs"), "{printed}");
    assert!(!printed.contains("src/other.rs"), "{printed}");
}

#[test]
fn the_binary_reports_paths_that_hold_no_file() {
    let repo = mini_repo();
    std::fs::create_dir_all(repo.path().join("docs")).expect("docs dir");

    let output = run(repo.path(), &["--path", "docs"]);
    let printed = stdout(&output);

    assert_eq!(output.status.code(), Some(2), "{printed}");
    assert!(
        printed.contains("the paths given hold no file"),
        "{printed}"
    );
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
