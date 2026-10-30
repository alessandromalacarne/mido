//! The mutation gate's default scope: the change a git diff describes.

use super::*;

/// The diff file the gate writes for a workspace run's change.
fn diff_path(run: &crate::gates::GateRun<'_>) -> std::path::PathBuf {
    run.scratch.join("guardrails-mutants-workspace.diff")
}

#[test]
fn the_pass_is_scoped_to_the_change_diff() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 3,
        caught: 3,
        ..Pass::default()
    })]);

    let files = files();
    let config = config_for(&repo);
    let run = gate_run_for(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, PASS);
    assert!(
        stub.called_exactly(&["git", "diff", "HEAD"]),
        "the change is the working tree against the last commit: {:?}",
        stub.git_calls()
    );
    let diff = diff_path(&run);
    let written = std::fs::read_to_string(&diff).expect("the diff file is written");
    assert_eq!(written, stub.diff_text());
    let expected = format!("--in-diff {}", diff.display());
    assert!(
        stub.called_with(&expected),
        "{expected}\nactual: {:?}",
        stub.mutant_calls()
    );
    assert!(result.contract.contains("--in-diff"), "{}", result.contract);
}

#[test]
fn a_change_outside_the_target_leaves_nothing_to_mutate() {
    let repo = repo();
    let stub = MutationStub::new(vec![]).change(&["frontend/src/foo.rs"]);

    let files = files();
    let config = config_for(&repo);
    let run = gate_run_for(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(
        result.status, PASS,
        "a file the target does not measure is no change to it: {}",
        result.summary
    );
    assert!(
        result.summary.contains("no rust file in the change"),
        "{}",
        result.summary
    );
    assert!(!stub.called_with("cargo mutants"));
}

#[test]
fn untracked_rust_files_ride_the_change_and_the_index_is_released() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 2,
        caught: 2,
        ..Pass::default()
    })])
    .answers(&[("ls-files --others", 0, "src/new.rs\nREADME.md\n")]);

    let files = files();
    let config = config_for(&repo);
    let run = gate_run_for(&repo, &config, &files);
    gate_mutation(&stub, &run);

    assert!(
        stub.called_exactly(&["git", "add", "-N", "src/new.rs"]),
        "a new source file joins the change — and only it: {:?}",
        stub.mutant_calls()
    );
    assert!(
        stub.called_exactly(&["git", "reset", "-q", "--", "src/new.rs"]),
        "the index is left as it was found"
    );
}

#[test]
fn a_change_that_yields_no_mutant_is_incomplete() {
    let repo = repo();
    let stub =
        MutationStub::new(vec![None]).answers(&[("cargo mutants", 0, "INFO No mutants to filter")]);

    let files = files();
    let config = config_for(&repo);
    let run = gate_run_for(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(
        result.status, INCOMPLETE,
        "a rust file with no mutant to filter is not a pass"
    );
    assert!(
        result.summary.contains("no mutant in the change"),
        "{}",
        result.summary
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("No mutants to filter")),
        "the tool's own tail rides along: {:?}",
        result.details
    );
}
