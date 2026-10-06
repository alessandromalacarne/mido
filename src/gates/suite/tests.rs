use super::*;
use crate::test_support::{config_for, gate_run_for, FakeRunner, MiniRepo};

fn repo(config: Option<&str>) -> MiniRepo {
    MiniRepo::build(config)
}

#[test]
fn failing_tests_are_named_in_the_tests_gate() {
    let repo = repo(Some(
        "
        version = 1

        [tests]
        command = [\"cargo\", \"test\", \"--all-features\"]
    ",
    ));
    let output = "test thing::works ... ok\ntest thing::breaks ... FAILED\n\ntest result: FAILED. 1 passed; 1 failed\n";
    let runner = FakeRunner::with(&[("cargo test", 101, output)]);

    let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, FAIL);
    assert!(result.details.join(" ").contains("thing::breaks"));
    assert!(result.contract.contains("cargo test --all-features"));
}

#[test]
fn a_green_suite_reports_its_counts() {
    let repo = repo(Some(
        "
        version = 1

        [tests]
        command = [\"cargo\", \"test\"]
    ",
    ));
    let output = "test result: ok. 41 passed; 0 failed; 0 ignored\n";
    let runner = FakeRunner::with(&[("cargo test", 0, output)]);

    let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, PASS);
    assert_eq!(result.summary, "cargo test: 41 passed, 0 failed");
}

#[test]
fn tests_gate_fails_when_the_command_prints_no_summary() {
    let repo = repo(Some(
        "
        version = 1

        [tests]
        command = [\"cargo\", \"test\"]
    ",
    ));
    let runner = FakeRunner::with(&[("cargo test", 127, "cargo: command not found")]);

    let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, FAIL);
    assert!(result.details.join(" ").contains("no test summary"));
}

#[test]
fn a_nonzero_exit_with_every_test_green_is_still_a_failure() {
    let repo = repo(Some(
        "
        version = 1

        [tests]
        command = [\"cargo\", \"test\"]
    ",
    ));
    let runner = FakeRunner::with(&[("cargo test", 101, "test result: ok. 3 passed; 0 failed\n")]);

    let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, FAIL);
    assert!(result.details.join(" ").contains("`cargo test`: 0 failed"));
}

#[test]
fn a_failing_suite_that_exits_zero_is_still_a_failure() {
    let repo = repo(Some(
        "
        version = 1

        [tests]
        command = [\"cargo\", \"test\"]
    ",
    ));
    let runner =
        FakeRunner::with(&[("cargo test", 0, "test result: FAILED. 0 passed; 1 failed\n")]);

    let result = gate_tests(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, FAIL);
    assert!(result.details.join(" ").contains("`cargo test`: 1 failed"));
}
