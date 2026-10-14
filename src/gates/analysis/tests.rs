use super::*;
use crate::test_support::{
    config_for, gate_run_for, js_config_for, js_gate_run_for, MiniJsRepo, MiniRepo,
};

fn repo(config: Option<&str>) -> MiniRepo {
    MiniRepo::build(config)
}

fn js_repo(config: Option<&str>) -> MiniJsRepo {
    MiniJsRepo::build(config)
}

fn unit(name: &str, mi: Option<f64>, cognitive: i64) -> Unit {
    Unit {
        mi,
        cognitive,
        ..Unit::new("src/foo.rs", name)
    }
}

/// The readings a gate judges: the units, and nothing else.
fn readings(units: &[Unit]) -> Measurements {
    Measurements {
        units: units.to_vec(),
        ..Measurements::default()
    }
}

#[test]
fn an_index_exactly_at_the_floor_passes() {
    let repo = repo(None);
    let units = vec![unit("floored", Some(20.0), 2)];

    let result = gate_analysis(
        &gate_run_for(&repo, &config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, PASS, "{:?}", result.details);
}

#[test]
fn the_worst_units_are_judged_and_named() {
    let repo = repo(None);
    let units = vec![unit("good", Some(80.0), 2), unit("hard", Some(12.0), 30)];

    let result = gate_analysis(
        &gate_run_for(&repo, &config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, FAIL);
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("worst MI: 12.0 (min 20) — hard")));
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("worst cognitive: 30 (max 15)")));
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("src/foo.rs: hard MI 12.0 (min 20)")));
}

#[test]
fn healthy_units_pass_with_their_numbers_reported() {
    let repo = repo(None);
    let units = vec![unit("good", Some(80.0), 2)];

    let result = gate_analysis(
        &gate_run_for(&repo, &config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, PASS);
    assert_eq!(result.summary, "worst MI 80.0, cognitive 2");
    assert!(result.contract.contains("mi_min=20, cognitive_max=15"));
}

#[test]
fn no_function_metrics_is_incomplete_never_a_pass() {
    let repo = repo(None);

    let result = gate_analysis(
        &gate_run_for(&repo, &config_for(&repo), &[]),
        &Measurements::default(),
    );

    assert_eq!(result.status, INCOMPLETE);
    assert_eq!(result.summary, "no function metrics to judge");
}

#[test]
fn the_first_tool_error_explains_the_incompleteness() {
    let repo = repo(None);
    let errors = vec!["src/foo.rs: rust-code-analysis could not read it (exit 1)".to_string()];
    let measurements = Measurements {
        errors: errors.clone(),
        ..Measurements::default()
    };

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &measurements);

    assert_eq!(result.status, INCOMPLETE);
    assert_eq!(result.summary, errors[0]);
    assert!(result.details.contains(&errors[0]));
}

#[test]
fn an_unsupported_analysis_tool_is_incomplete_with_a_pointer() {
    let repo = repo(Some(
        "
        version = 1

        [analysis]
        tool = \"lizard\"
    ",
    ));

    let result = gate_analysis(
        &gate_run_for(&repo, &config_for(&repo), &[]),
        &Measurements::default(),
    );

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("`lizard` is not supported"));
    assert!(result.fixes[0].contains("point [analysis] tool at rust-code-analysis"));
}

#[test]
fn a_measurement_error_with_units_present_is_still_incomplete() {
    let repo = repo(None);
    let units = vec![unit("good", Some(80.0), 2)];
    let measurements = Measurements {
        errors: vec!["src/a.js: Parsing error: ':' expected.".to_string()],
        ..readings(&units)
    };

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &measurements);

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("Parsing error"));
}

#[test]
fn a_stack_without_a_maintainability_index_is_judged_on_cognitive_alone() {
    let repo = js_repo(None);
    let units = vec![unit("busy", None, 30), unit("calm", None, 2)];

    let result = gate_analysis(
        &js_gate_run_for(&repo, &js_config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, FAIL);
    assert_eq!(result.summary, "cognitive 30 (no MI in this stack)");
    assert!(result.contract.contains("cognitive_max=15"));
    assert!(!result.contract.contains("mi_min"));
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("MI: not measured (eslint reports no maintainability index)")));
}

#[test]
fn a_configured_mi_floor_without_readings_is_incomplete() {
    let repo = js_repo(Some(
        "
        version = 1

        [analysis]
        mi_min = 65
    ",
    ));
    let units = vec![unit("busy", None, 3)];

    let result = gate_analysis(
        &js_gate_run_for(&repo, &js_config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("no maintainability index"));
    assert!(result.fixes[0].contains("drop [analysis] mi_min"));
}

#[test]
fn a_partial_missing_index_is_incomplete_when_the_floor_is_configured() {
    let repo = repo(None);
    let units = vec![unit("measured", Some(60.0), 2), unit("unmeasured", None, 2)];

    let result = gate_analysis(
        &gate_run_for(&repo, &config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, INCOMPLETE);
    assert!(result
        .summary
        .contains("1 of 2 functions have no maintainability index"));
}

#[test]
fn a_floor_nobody_set_is_not_enforced() {
    let repo = js_repo(None);
    // 12.0 would fail the code-level default of 20: only a set floor counts.
    let units = vec![unit("hard", Some(12.0), 2)];

    let result = gate_analysis(
        &js_gate_run_for(&repo, &js_config_for(&repo), &[]),
        &readings(&units),
    );

    assert_eq!(result.status, PASS);
    assert!(!result.contract.contains("mi_min"));
}
