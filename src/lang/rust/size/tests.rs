use super::*;
use crate::test_support::{FakeRunner, MiniRepo};

#[test]
fn tokei_counts_come_from_the_report() {
    let repo = MiniRepo::build(None);
    let tokei = serde_json::json!({
        "Rust": {
            "code": 12,
            "reports": [
                { "name": "src/foo.rs", "stats": { "code": 12 } }
            ]
        },
        "Total": { "code": 12, "reports": [{ "name": "Total", "stats": { "code": 12 } }] }
    });
    let runner = FakeRunner::with(&[("tokei", 0, &tokei.to_string())]);
    let mut errors = Vec::new();

    let counts = code_lines(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "tokei",
        &mut errors,
    );

    assert_eq!(counts.get("src/foo.rs"), Some(&12));
    assert!(!counts.contains_key("Total"));
    assert!(errors.is_empty());
}

#[test]
fn an_unsupported_size_tool_is_an_error_not_a_silent_pass() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::default();
    let mut errors = Vec::new();

    let counts = code_lines(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "scc",
        &mut errors,
    );

    assert!(counts.is_empty());
    assert!(errors[0].contains("`scc` is not supported"));
}

#[test]
fn unreadable_tokei_output_is_an_error() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("tokei", 0, "not json")]);
    let mut errors = Vec::new();

    code_lines(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "tokei",
        &mut errors,
    );

    assert!(errors.iter().any(|error| error.contains("no usable json")));
}

#[test]
fn a_failing_tokei_run_is_an_error_even_with_partial_output() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("tokei", 1, "{}")]);
    let mut errors = Vec::new();

    let counts = code_lines(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "tokei",
        &mut errors,
    );

    assert!(counts.is_empty());
    assert!(errors[0].contains("could not measure"));
}
