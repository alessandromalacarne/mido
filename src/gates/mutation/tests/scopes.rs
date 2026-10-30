//! What a run scopes, and what it refuses to run.

use super::*;

#[test]
fn a_command_that_names_its_own_diff_is_incomplete() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        command = [\"cargo\", \"mutants\", \"--in-diff\", \"theirs\"]
    ",
    ));
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("--in-diff"),
        "mido scopes the change itself, and says so: {}",
        result.summary
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("in_diff = false")),
        "the hint names the way out: {:?}",
        result.details
    );
    assert!(!stub.called_with("cargo mutants"));
}

#[test]
fn the_mutant_timeout_is_passed_through() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert!(result.contract.contains("--timeout 120"));
}
