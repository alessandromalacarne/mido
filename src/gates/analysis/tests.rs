use super::*;
use crate::test_support::{config_for, gate_run_for, MiniRepo};

fn repo(config: Option<&str>) -> MiniRepo {
    MiniRepo::build(config)
}

fn unit(name: &str, mi: f64, cognitive: i64) -> Unit {
    Unit {
        mi,
        cognitive,
        ..Unit::new("src/foo.rs", name)
    }
}

#[test]
fn the_worst_units_are_judged_and_named() {
    let repo = repo(None);
    let units = vec![unit("good", 80.0, 2), unit("hard", 12.0, 30)];

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &units, &[]);

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
    let units = vec![unit("good", 80.0, 2)];

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &units, &[]);

    assert_eq!(result.status, PASS);
    assert_eq!(result.summary, "worst MI 80.0, cognitive 2");
    assert!(result.contract.contains("mi_min=20, cognitive_max=15"));
}

#[test]
fn no_function_metrics_is_incomplete_never_a_pass() {
    let repo = repo(None);

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &[], &[]);

    assert_eq!(result.status, INCOMPLETE);
    assert_eq!(result.summary, "no function metrics to judge");
}

#[test]
fn the_first_tool_error_explains_the_incompleteness() {
    let repo = repo(None);
    let errors = vec!["src/foo.rs: rust-code-analysis could not read it (exit 1)".to_string()];

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &[], &errors);

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

    let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo), &[]), &[], &[]);

    assert_eq!(result.status, INCOMPLETE);
    assert!(result.summary.contains("`lizard` is not supported"));
    assert!(result.fixes[0].contains("point [analysis] tool at rust-code-analysis"));
}
