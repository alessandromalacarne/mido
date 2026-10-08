//! What a run scopes — the whole package — and what it refuses to run.

use super::*;

#[test]
fn a_configured_output_directory_is_incomplete() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        command = [\"cargo\", \"mutants\", \"--output\", \"theirs\"]
    ",
    ));
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("--output"),
        "mido owns the mutation state, and says so: {}",
        result.summary
    );
    assert!(!stub.called_with("cargo mutants"));
}

#[test]
fn a_short_output_flag_is_incomplete_too() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        command = [\"cargo\", \"mutants\", \"-o\", \"theirs\"]
    ",
    ));
    let config = config_for(&repo);
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config, &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("--output"), "{}", result.summary);
    assert!(!stub.called_with("cargo mutants"));
}

#[test]
fn the_mutant_timeout_is_passed_through() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert!(result.contract.contains("--timeout 120"));
}

#[test]
fn the_package_is_the_scope_and_no_patch_or_file_flag_rides_along() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    for flag in ["--in-diff", "--file", "scope="] {
        assert!(
            !result.contract.contains(flag),
            "`{flag}` is gone: {}",
            result.contract
        );
    }
}

#[test]
fn a_target_without_a_rust_file_never_runs_the_mutants() {
    let repo = repo();
    let stub = MutationStub::new(vec![]);

    let files = vec!["README.md".to_string()];
    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files));

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("no rust file in the target"),
        "{}",
        result.summary
    );
    assert!(!stub.called_with("cargo mutants"));
}
