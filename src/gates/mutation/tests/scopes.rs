//! What a run scopes and which argv it builds — and what it refuses to run.

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

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &[]));

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

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config, &[]));

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("--output"), "{}", result.summary);
    assert!(!stub.called_with("cargo mutants"));
}

#[test]
fn the_scope_setting_can_name_a_path_instead_of_the_diff() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        scope = \"src/parser.rs\"
    ",
    ));
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert!(result.contract.contains("--file src/parser.rs"));
    assert!(result.contract.contains("scope=src/parser.rs"));
}

#[test]
fn the_all_scope_adds_no_scope_flags() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        scope = \"all\"
    ",
    ));
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert!(!result.contract.contains("--in-place"));
    assert!(!result.contract.contains("--in-diff"));
    assert!(
        !result.contract.contains("--file"),
        "`all` scopes nothing: no path may ride along: {}",
        result.contract
    );
}

#[test]
fn the_mutant_timeout_is_passed_through() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert!(result.contract.contains("--timeout 120"));
}

#[test]
fn explicit_paths_scope_the_mutants_to_the_named_files() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);
    let changed = vec!["src/foo.rs".to_string(), "README.md".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.scope = Scope::Paths;
    let result = gate_mutation(&stub, &run);

    assert!(
        result.contract.contains("--file src/foo.rs"),
        "{}",
        result.contract
    );
    assert!(
        !result.contract.contains("--in-diff"),
        "{}",
        result.contract
    );
    assert!(
        !result.contract.contains("README.md"),
        "{}",
        result.contract
    );
}

#[test]
fn a_patch_that_cannot_be_written_is_incomplete_never_a_pass() {
    let repo = repo();
    let stub = MutationStub::new(vec![]).answers(&[(
        "git diff",
        0,
        "diff --git a/src/foo.rs b/src/foo.rs\n",
    )]);
    let scratch = repo.root.join("missing/scratch");
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &[]);
    run.scratch = &scratch;

    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("patch could not be written"),
        "{:?}",
        result.summary
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("missing/scratch")),
        "{:?}",
        result.details
    );
}

#[test]
fn explicit_paths_without_a_rust_file_never_run_the_mutants() {
    let repo = repo();
    let stub = MutationStub::new(vec![]);

    let changed = vec!["README.md".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.scope = Scope::Paths;
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("no rust file in the paths given"));
    assert!(!stub.called_with("cargo mutants"));
}

#[test]
fn a_whole_target_scope_mutates_the_package_without_a_patch() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);
    let changed = vec!["src/foo.rs".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.scope = Scope::Whole;
    let result = gate_mutation(&stub, &run);

    assert!(
        !result.contract.contains("--in-diff"),
        "no diff, no patch: {}",
        result.contract
    );
    assert!(
        !result.contract.contains("--file"),
        "the package is the scope: {}",
        result.contract
    );
    assert!(
        result.contract.contains("--timeout 120"),
        "{}",
        result.contract
    );
}

#[test]
fn a_whole_target_without_a_rust_file_never_runs_the_mutants() {
    let repo = repo();
    let stub = MutationStub::new(vec![]);

    let changed = vec!["README.md".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.scope = Scope::Whole;
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("no rust file in the target"),
        "{}",
        result.summary
    );
    assert!(!stub.called_with("cargo mutants"));
}
