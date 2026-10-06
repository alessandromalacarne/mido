use super::*;
use crate::test_support::{config_for, gate_run_for, FakeRunner, MiniRepo};

fn repo() -> MiniRepo {
    MiniRepo::build(None)
}

const SUMMARY: &str = "120 mutants tested in 3m: 10 missed, 105 caught, 5 unviable\n";

#[test]
fn a_summary_that_omits_the_zero_categories_is_read() {
    let repo = repo();
    let output = "115 mutants tested in 6m: 96 caught, 19 unviable\n";
    let runner = FakeRunner::with(&[("cargo mutants", 0, output)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("100.0% killed (min 70)"));
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("96 caught, 0 missed, 19 unviable")));
}

#[test]
fn a_kill_rate_above_the_minimum_passes_and_reports_the_numbers() {
    let repo = repo();
    let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("91.3% killed (min 70)"));
    assert!(result.details[0].contains("120 mutants: 105 caught, 10 missed, 5 unviable"));
    assert!(
        result.details[0].contains("-> 91.3% killed"),
        "the details line repeats the rate: {:?}",
        result.details[0]
    );
    assert!(
        !result.details[0].contains("skipped"),
        "no note when nothing was skipped: {:?}",
        result.details[0]
    );
}

#[test]
fn a_kill_rate_exactly_at_the_minimum_passes() {
    let repo = repo();
    let output = "10 mutants tested in 1m: 3 missed, 7 caught\n";
    let runner = FakeRunner::with(&[("cargo mutants", 0, output)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("70.0% killed (min 70)"));
}

#[test]
fn a_kill_rate_below_the_minimum_fails_and_lists_the_survivors() {
    let repo = repo();
    let output =
        "100 mutants tested in 3m: 60 missed, 40 caught, 0 unviable\nMISSED  src/foo.rs:12:5 replace + with - in parse\n";
    let runner = FakeRunner::with(&[("cargo mutants", 0, output)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, FAIL);
    assert!(result.summary.contains("40.0% killed (min 70)"));
    assert!(result
        .details
        .iter()
        .any(|line| line.starts_with("MISSED  src/foo.rs")));
}

#[test]
fn an_endless_run_times_out_as_incomplete() {
    let repo = repo();
    let runner = FakeRunner::with(&[("cargo mutants", 124, "still going")]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("timed out after 3600s"));
}

#[test]
fn output_without_a_summary_is_incomplete() {
    let repo = repo();
    let runner = FakeRunner::with(&[("cargo mutants", 0, "cargo-mutants: nothing to do")]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, INCOMPLETE);
    assert_eq!(result.summary, "cargo-mutants produced no summary");
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
    let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

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
    let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

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
    let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert!(result.contract.contains("--timeout 120"));
}

#[test]
fn an_iterated_run_counts_the_skipped_mutants_as_killed() {
    let repo = repo();
    let output = " INFO Iteration excludes 348 previously caught or unviable mutants\nFound 2 mutants to test\n2 mutants tested in 49s: 2 missed\n";
    let runner = FakeRunner::with(&[("cargo mutants", 0, output)]);

    let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("99.4% killed (min 70)"));
    assert!(
        result.details[0].contains("350 mutants"),
        "the skipped mutants count into the total: {:?}",
        result.details[0]
    );
    assert!(
        result.details[0].contains("-> 99.4% killed"),
        "the details line repeats the rate: {:?}",
        result.details[0]
    );
    assert!(
        result.details[0].contains("348 previously caught or unviable (skipped)"),
        "{:?}",
        result.details[0]
    );
}

#[test]
fn explicit_paths_scope_the_mutants_to_the_named_files() {
    let repo = repo();
    let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);
    let changed = vec!["src/foo.rs".to_string(), "README.md".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.scope = Scope::Paths;
    let result = gate_mutation(&runner, &run);

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
    let runner = FakeRunner::with(&[("git diff", 0, "diff --git a/src/foo.rs b/src/foo.rs\n")]);
    let scratch = repo.root.join("missing/scratch");
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &[]);
    run.scratch = &scratch;

    let result = gate_mutation(&runner, &run);

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
    let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

    let changed = vec!["README.md".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.scope = Scope::Paths;
    let result = gate_mutation(&runner, &run);

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("no rust file in the paths given"));
    assert!(!runner.called_with("cargo mutants"));
}
