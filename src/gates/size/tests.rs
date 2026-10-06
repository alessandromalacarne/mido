use super::*;
use crate::test_support::{config_for, gate_run_for, touch, FakeRunner, MiniRepo};

fn limits() -> SizeLimits {
    SizeLimits {
        file_loc: Threshold::new(300, 500),
        function_loc: Threshold::new(40, 60),
        complexity: Threshold::new(10, 15),
        nesting: Threshold::new(3, 4),
    }
}

fn unit(name: &str, sloc: i64) -> Unit {
    Unit {
        sloc,
        name: name.to_string(),
        path: "src/foo.rs".to_string(),
        ..Unit::new("src/foo.rs", name)
    }
}

#[test]
fn file_sizes_fail_at_the_ceiling_and_warn_before_it() {
    let lines = BTreeMap::from([
        ("src/big.rs".to_string(), 500),
        ("src/medium.rs".to_string(), 300),
        ("src/small.rs".to_string(), 10),
    ]);

    let (problems, details) = judge_file_sizes(&lines, &limits());

    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("src/big.rs: 500 code lines (fail >= 500)"));
    assert!(details
        .iter()
        .any(|line| line.contains("src/medium.rs: 300 code lines (warn >= 300)")));
    assert!(details
        .iter()
        .any(|line| line.contains("src/small.rs: 10 code lines (ok)")));
}

#[test]
fn function_metrics_fail_on_length_complexity_and_nesting() {
    let long = unit("long", 61);
    let complex = Unit {
        cyclomatic: 15,
        ..unit("complex", 10)
    };
    let nested = Unit {
        nesting: Some(4),
        ..unit("nested", 10)
    };

    let (problems, near) = judge_function_metrics(&[long, complex, nested], &limits());

    assert_eq!(problems.len(), 3);
    assert!(problems
        .iter()
        .any(|line| line.contains("function_loc 61 (fail >= 60)")));
    assert!(problems
        .iter()
        .any(|line| line.contains("complexity 15 (fail >= 15)")));
    assert!(problems
        .iter()
        .any(|line| line.contains("nesting 4 (fail >= 4)")));
    assert!(near.is_empty());
}

#[test]
fn a_near_ceiling_function_warns_without_failing() {
    let (problems, near) = judge_function_metrics(&[unit("borderline", 45)], &limits());

    assert!(problems.is_empty());
    assert_eq!(near.len(), 1);
    assert!(near[0].contains("near the ceiling: borderline"));
    assert!(near[0].contains("function_loc(warn)"));
}

#[test]
fn an_unmeasured_nesting_does_not_judge_nesting() {
    let (problems, _) = judge_function_metrics(&[unit("no-nesting-data", 10)], &limits());

    assert!(problems.is_empty());
}

#[test]
fn a_function_over_two_ceilings_reports_both_but_is_not_near_one() {
    let over = Unit {
        cyclomatic: 20,
        ..unit("over", 70)
    };

    let (problems, near) = judge_function_metrics(&[over], &limits());

    assert_eq!(problems.len(), 2);
    assert!(near.is_empty());
}

#[test]
fn the_gate_fails_when_a_changed_file_is_over_the_ceiling() {
    let repo = MiniRepo::build(None);
    touch(&repo, "src/foo.rs");
    let config = config_for(&repo);
    let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 600 } }] } });
    let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
    let units = vec![unit("small", 5)];

    let changed = vec!["src/foo.rs".to_string()];
    let result = gate_size(
        &runner,
        &gate_run_for(&repo, &config, &changed),
        &units,
        &[],
    );

    assert_eq!(result.status, FAIL);
    assert!(result.summary.contains("worst function"));
    assert!(result.contract.contains("file_loc.fail=500"));
}

#[test]
fn the_gate_is_incomplete_when_the_metrics_never_arrived() {
    let repo = MiniRepo::build(None);
    touch(&repo, "src/foo.rs");
    let config = config_for(&repo);
    let runner = FakeRunner::with(&[("tokei", 127, "")]);

    let changed = vec!["src/foo.rs".to_string()];
    let result = gate_size(&runner, &gate_run_for(&repo, &config, &changed), &[], &[]);

    assert_eq!(result.status, INCOMPLETE);
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("measurement:")));
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("returned nothing")));
}

#[test]
fn a_change_without_rust_files_passes_as_nothing_to_measure() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let runner = FakeRunner::default();

    let changed = vec!["README.md".to_string()];
    let result = gate_size(&runner, &gate_run_for(&repo, &config, &changed), &[], &[]);

    assert_eq!(result.status, PASS);
    assert_eq!(result.summary, "no rust changes");
    assert!(
        !result
            .details
            .iter()
            .any(|line| line.contains("returned nothing")),
        "nothing was measured, so nothing can be reported missing: {:?}",
        result.details
    );
}

#[test]
fn a_measurement_error_with_units_present_is_still_incomplete() {
    let repo = MiniRepo::build(None);
    touch(&repo, "src/foo.rs");
    let config = config_for(&repo);
    let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
    let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
    let errors = vec!["src/bar.rs: rust-code-analysis could not read it".to_string()];

    let changed = vec!["src/foo.rs".to_string()];
    let result = gate_size(
        &runner,
        &gate_run_for(&repo, &config, &changed),
        &[unit("f", 5)],
        &errors,
    );

    assert_eq!(result.status, INCOMPLETE);
}

#[test]
fn missing_units_do_not_claim_nesting_was_absent() {
    let repo = MiniRepo::build(None);
    touch(&repo, "src/foo.rs");
    let config = config_for(&repo);
    let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
    let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);

    let changed = vec!["src/foo.rs".to_string()];
    let result = gate_size(&runner, &gate_run_for(&repo, &config, &changed), &[], &[]);

    assert!(result
        .details
        .iter()
        .any(|line| line.contains("returned nothing")));
    assert!(
        !result
            .details
            .iter()
            .any(|line| line.contains("nesting: not measured")),
        "no units arrived, so nesting was never looked at: {:?}",
        result.details
    );
}

#[test]
fn nesting_absence_is_stated_in_the_evidence() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 5 } }] } });
    let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);

    let changed = vec!["src/foo.rs".to_string()];
    let result = gate_size(
        &runner,
        &gate_run_for(&repo, &config, &changed),
        &[unit("f", 5)],
        &[],
    );

    assert_eq!(result.status, PASS);
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("nesting: not measured")));
}
