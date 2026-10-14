use super::*;
use crate::test_support::{FakeRunner, MiniRepo};

fn files() -> Vec<String> {
    vec!["src/foo.rs".to_string()]
}

#[test]
fn the_pass_answers_with_tokei_lines_and_rust_code_analysis_units() {
    let repo = MiniRepo::build(None);
    let tokei = serde_json::json!({
        "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 12 } }] }
    });
    let document = serde_json::json!({
        "spaces": [{ "name": "f", "kind": "function", "metrics": { "loc": { "sloc": 5 }, "mi": { "mi_original": 40.0 } } }]
    });
    let runner = FakeRunner::with(&[
        ("rust-code-analysis-cli", 0, &document.to_string()),
        ("tokei", 0, &tokei.to_string()),
    ]);

    let measurements = measure(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &files(),
        "cargo",
    );

    assert_eq!(measurements.files, files());
    assert_eq!(measurements.lines["src/foo.rs"], 12);
    assert_eq!(measurements.units.len(), 1);
    assert_eq!(measurements.units[0].name, "f");
    assert_eq!(measurements.units[0].sloc, 5);
    assert_eq!(measurements.units[0].mi, Some(40.0));
    assert!(measurements.errors.is_empty(), "{:?}", measurements.errors);
}

#[test]
fn a_tool_that_could_not_read_a_file_says_so() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("tokei", 127, "")]);

    let measurements = measure(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &files(),
        "cargo",
    );

    assert!(measurements.lines.is_empty());
    assert!(measurements
        .errors
        .iter()
        .any(|error| { error.contains("tokei") && error.contains("could not measure") }));
    assert!(measurements.errors.iter().any(|error| {
        error.contains("rust-code-analysis") && error.contains("could not read it")
    }));
}

#[test]
fn nothing_to_measure_runs_nothing() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::default();

    let measurements = measure(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &[],
        "cargo",
    );

    assert!(measurements.errors.is_empty());
    assert!(!runner.called_with("tokei"));
    assert!(!runner.called_with("rust-code-analysis-cli"));
}
