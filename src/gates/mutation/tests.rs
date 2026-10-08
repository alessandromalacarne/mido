use super::*;
use crate::report::{FAIL, PASS};
use crate::test_support::{config_for, gate_run_for, MiniRepo};
use stub::{write_pass, MutationStub, Pass};

mod passes;
mod scopes;
mod stub;

fn repo() -> MiniRepo {
    MiniRepo::build(None)
}

/// The measured rust file every mutation run needs to reach cargo-mutants.
fn files() -> Vec<String> {
    vec!["src/foo.rs".to_string()]
}

#[test]
fn a_finished_run_decides_the_verdict() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 115,
        caught: 96,
        unviable: 19,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("100.0% killed (min 70)"));
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("115 mutants: 96 caught, 0 missed, 19 unviable")));
}

#[test]
fn a_kill_rate_above_the_minimum_passes_and_reports_the_numbers() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 120,
        caught: 105,
        missed: 10,
        unviable: 5,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

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
    assert!(
        !result.details.iter().any(|line| line.starts_with("pass `")),
        "a single-pass run keeps the shorter evidence: {:?}",
        result.details
    );
}

#[test]
fn a_kill_rate_exactly_at_the_minimum_passes() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 10,
        caught: 7,
        missed: 3,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("70.0% killed (min 70)"));
}

#[test]
fn a_kill_rate_below_the_minimum_fails_and_lists_the_survivors() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 100,
        caught: 40,
        missed: 60,
        survivors: vec!["src/foo.rs:12:5: replace + with - in parse"],
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, FAIL);
    assert!(result.summary.contains("40.0% killed (min 70)"));
    assert!(result
        .details
        .iter()
        .any(|line| line.starts_with("MISSED  src/foo.rs")));
}

#[test]
fn an_iterated_pass_counts_the_excluded_mutants_as_killed() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 2,
        missed: 2,
        skipped: vec!["a", "b", "c", "d", "e", "f", "g", "h"],
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, PASS);
    assert!(result.summary.contains("80.0% killed (min 70)"));
    assert!(
        result.details[0].contains("10 mutants"),
        "the skipped mutants count into the total: {:?}",
        result.details[0]
    );
    assert!(
        result.details[0].contains("8 previously caught or unviable (skipped)"),
        "{:?}",
        result.details[0]
    );
    assert!(
        result.details[0].contains("-> 80.0% killed"),
        "the counts line repeats the rate the verdict earned: {:?}",
        result.details[0]
    );
}

#[test]
fn an_endless_run_times_out_as_incomplete() {
    let repo = repo();
    let stub = MutationStub::new(vec![]).answers(&[("cargo mutants", 124, "still going")]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("timed out after 3600s"));
}

#[test]
fn a_pass_that_wrote_no_report_is_incomplete() {
    let repo = repo();
    let stub = MutationStub::new(vec![None]).answers(&[(
        "cargo mutants",
        0,
        "cargo-mutants: nothing to do",
    )]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert_eq!(result.summary, "cargo-mutants produced no report");
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("nothing to do")),
        "the tool's own tail rides along: {:?}",
        result.details
    );
}

#[test]
fn a_timed_out_mutant_is_incomplete() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 5,
        caught: 4,
        timeout: 1,
        timed_out: vec!["src/foo.rs:9:9: replace a with b in slow"],
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE, "a timeout is not a verdict");
    assert!(
        result
            .summary
            .contains("1 of 5 mutants produced no verdict"),
        "{}",
        result.summary
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.starts_with("TIMEOUT  src/foo.rs:9:9")),
        "{:?}",
        result.details
    );
    assert!(
        !result
            .details
            .iter()
            .any(|line| line.contains("did not report an outcome")),
        "every unresolved mutant was named: {:?}",
        result.details
    );
}

#[test]
fn an_unclassified_mutant_is_incomplete() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 4,
        caught: 2,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(
        result.status, INCOMPLETE,
        "a mutant the tool could not classify is no verdict"
    );
    assert!(
        result
            .summary
            .contains("2 of 4 mutants produced no verdict"),
        "{}",
        result.summary
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("2 mutants did not report an outcome")),
        "{:?}",
        result.details
    );
}

#[test]
fn a_failed_baseline_is_incomplete() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 3,
        caught: 3,
        baseline_failure: Some("Failure"),
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("baseline") && result.summary.contains("Failure"),
        "{}",
        result.summary
    );
}

#[test]
fn a_stale_report_from_an_earlier_run_is_never_read() {
    let repo = repo();
    let config = config_for(&repo);
    let files = files();
    let run = gate_run_for(&repo, &config, &files);
    // A leftover report where the gate keeps its state: were it read, this
    // valid-looking pass would decide the verdict.
    write_pass(
        &run.scratch.join("guardrails-mutants-workspace"),
        &Pass {
            total: 63,
            caught: 63,
            ..Pass::default()
        },
    );
    let stub = MutationStub::new(vec![None]);

    let result = gate_mutation(&stub, &run);

    assert_eq!(
        result.status, INCOMPLETE,
        "the verdict must be earned by this run, not an earlier one"
    );
    assert!(result.summary.contains("produced no report"));
}

#[test]
fn the_pass_names_its_own_output_directory() {
    let repo = repo();
    let scratch = repo.root.join("scratch");
    std::fs::create_dir_all(&scratch).expect("scratch");
    let config = config_for(&repo);
    let files = files();
    let mut run = gate_run_for(&repo, &config, &files);
    run.scratch = &scratch;
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    gate_mutation(&stub, &run);

    let expected = format!(
        "--output {}",
        scratch.join("guardrails-mutants-workspace").display()
    );
    assert!(
        stub.called_with(&expected),
        "{expected}\nactual: {:?}",
        stub.mutant_calls()
    );
}
