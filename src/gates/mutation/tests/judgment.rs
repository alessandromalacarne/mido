//! What a pass's report earns: verdicts only the report can decide.

use super::*;

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
fn a_run_with_as_many_caught_as_missed_is_still_judged() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass {
        total: 10,
        caught: 5,
        missed: 5,
        ..Pass::default()
    })]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(
        result.status, FAIL,
        "5 of 10 killed is a 50% kill rate, not a run with nothing to measure"
    );
    assert!(result.summary.contains("50.0% killed (min 70)"));
}

#[test]
fn a_run_with_no_mutants_at_all_is_never_a_percentage() {
    let repo = repo();
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    let result = gate_mutation(&stub, &gate_run_for(&repo, &config_for(&repo), &files()));

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result.summary.contains("no mutant to measure"),
        "{}",
        result.summary
    );
}
