use super::*;
use crate::test_support::{config_for, gate_run_for, FakeRunner, MiniRepo};

fn repo() -> MiniRepo {
    MiniRepo::build(None)
}

#[test]
fn every_reported_diagnostic_fails_the_gate() {
    let repo = repo();
    let clippy = "warning: unused variable: `x`\n  --> src/components/foo.rs:12:5\nwarning: pre-existing\n  --> src/legacy/old.rs:3:1\n";
    let runner =
        FakeRunner::with(&[("clippy", 1, clippy), ("fmt", 0, ""), ("check", 0, "")]).tool("cargo");
    let files = vec![
        "src/components/foo.rs".to_string(),
        "src/legacy/old.rs".to_string(),
    ];

    let result = gate_syntax(&runner, &gate_run_for(&repo, &config_for(&repo), &files));

    assert_eq!(result.name, "syntax");
    assert_eq!(result.status, FAIL);
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("src/components/foo.rs:12:5")));
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("src/legacy/old.rs:3:1")),
        "no diagnostic is out of scope: {:?}",
        result.details
    );
}

#[test]
fn the_same_diagnostic_from_clippy_and_check_is_counted_once() {
    let repo = repo();
    let clippy = "error: unused variable: `x`\n  --> src/foo.rs:9:9\n";
    let check = "warning: unused variable: `x`\n  --> src/foo.rs:9:9\n";
    let runner = FakeRunner::with(&[("clippy", 1, clippy), ("fmt", 0, ""), ("check", 1, check)])
        .tool("cargo");
    let files = vec!["src/foo.rs".to_string()];

    let result = gate_syntax(&runner, &gate_run_for(&repo, &config_for(&repo), &files));

    assert_eq!(result.status, FAIL);
    assert_eq!(
        result
            .details
            .iter()
            .filter(|line| line.contains("unused variable"))
            .count(),
        1
    );
}

#[test]
fn a_clean_run_reports_clean() {
    let repo = repo();
    let runner =
        FakeRunner::with(&[("fmt", 0, ""), ("check", 0, ""), ("clippy", 0, "")]).tool("cargo");
    let files = vec!["src/foo.rs".to_string()];

    let result = gate_syntax(&runner, &gate_run_for(&repo, &config_for(&repo), &files));

    assert_eq!(result.status, PASS);
    assert_eq!(result.summary, "clean");
}

#[test]
fn unattributable_linter_output_is_not_a_pass() {
    let repo = repo();
    let runner = FakeRunner::with(&[
        (
            "clippy",
            1,
            "error: something exploded in a way this runner cannot attribute\n",
        ),
        ("fmt", 0, ""),
        ("check", 0, ""),
    ])
    .tool("cargo");
    let files = vec!["src/foo.rs".to_string()];

    let result = gate_syntax(&runner, &gate_run_for(&repo, &config_for(&repo), &files));

    assert_eq!(result.status, FAIL);
    assert!(result.details.join(" ").contains("cannot attribute"));
}
