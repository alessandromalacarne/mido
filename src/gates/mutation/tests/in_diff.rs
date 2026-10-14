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
    })])
    .answers(&[
        ("rev-parse --verify", 0, "c0ffee\n"),
        ("merge-base", 0, "c0ffee\n"),
    ]);

    let files = files();
    let config = config_for(&repo);
    let run = gate_run_for(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, PASS);
    assert!(
        stub.called_with("git diff c0ffee"),
        "the change is the diff from the merge base with the default branch"
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
fn a_change_without_a_rust_file_passes_without_running_the_mutants() {
    let repo = repo();
    let stub = MutationStub::new(vec![]).change(&["README.md"]);

    let files = files();
    let config = config_for(&repo);
    let run = gate_run_for(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, PASS);
    assert!(
        result.summary.contains("no rust file in the change"),
        "{}",
        result.summary
    );
    assert!(
        !stub.called_with("cargo mutants"),
        "nothing to mutate never reaches cargo-mutants: {:?}",
        stub.mutant_calls()
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("guardrails-mutants-workspace.diff")),
        "the evidence names the diff that decided it: {:?}",
        result.details
    );
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
fn a_member_target_diff_stays_workspace_root_relative() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 2,
        caught: 2,
        ..Pass::default()
    })])
    .change(&["frontend/src/foo.rs"]);
    let target = Box::leak(Box::new(crate::targets::Target::crate_target(
        "frontend",
        true,
        crate::lang::rust::MANIFEST,
    )));

    let files = files();
    let config = config_for(&repo);
    let run = crate::test_support::gate_run(&repo.root, target, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, PASS, "{}", result.summary);
    assert!(
        stub.called_with("git diff HEAD -- frontend"),
        "the target is scoped by a pathspec, never --relative: {:?}",
        stub.mutant_calls()
    );
    let written = std::fs::read_to_string(run.scratch.join("guardrails-mutants-frontend.diff"))
        .expect("the member's diff file is written");
    assert!(
        written.contains("+++ b/frontend/src/foo.rs"),
        "the paths stay what cargo-mutants resolves them against: {written}"
    );
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
fn in_diff_false_sweeps_the_whole_target() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        in_diff = false
    ",
    ));
    let stub = MutationStub::new(vec![Some(Pass {
        total: 3,
        caught: 3,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, PASS);
    assert!(
        !stub.called_with("git diff"),
        "no change diff is generated: {:?}",
        stub.mutant_calls()
    );
    assert!(
        !stub.called_with("--in-diff"),
        "the whole target is the scope: {:?}",
        stub.mutant_calls()
    );
}

#[test]
fn a_command_may_name_its_own_diff_when_in_diff_is_false() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        command = [\"cargo\", \"mutants\", \"--in-diff\", \"theirs\"]
        in_diff = false
    ",
    ));
    let stub = MutationStub::new(vec![Some(Pass {
        total: 3,
        caught: 3,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(
        result.status, PASS,
        "with mido's scope off, the command's own diff is the contract: {}",
        result.summary
    );
    assert!(stub.called_with("--in-diff theirs"));
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
