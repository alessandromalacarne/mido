use super::*;
use crate::test_support::{FakeRunner, MiniRepo};

#[test]
fn a_document_yields_its_functions_at_any_depth() {
    let document = serde_json::json!({
        "name": "src/foo.rs",
        "metrics": {},
        "spaces": [
            {
                "name": "outer",
                "kind": "function",
                "metrics": { "loc": { "sloc": 12 }, "cyclomatic": { "sum": 3 }, "cognitive": { "sum": 2 }, "mi": { "mi_original": 55.5 } },
                "spaces": [
                    { "name": "inner", "kind": "method", "metrics": { "loc": { "sloc": 4 } } }
                ]
            },
            { "name": "mod", "kind": "unit", "spaces": [] }
        ]
    });

    let units = units_from_document("src/foo.rs", &document);

    assert_eq!(units.len(), 2);
    assert_eq!(units[0].name, "outer");
    assert_eq!(units[0].sloc, 12);
    assert_eq!(units[0].cyclomatic, 3);
    assert_eq!(units[0].cognitive, 2);
    assert_eq!(units[0].mi, Some(55.5));
    assert_eq!(units[1].name, "inner");
    assert_eq!(units[1].path, "src/foo.rs");
}

#[test]
fn nesting_comes_from_whichever_family_the_version_prints() {
    let document = serde_json::json!({
        "spaces": [{ "name": "f", "kind": "function", "metrics": { "nested_control_flow": { "sum": 3 } } }]
    });

    assert_eq!(units_from_document("a.rs", &document)[0].nesting, Some(3));

    let document = serde_json::json!({
        "spaces": [{ "name": "f", "kind": "function", "metrics": { "nesting": { "sum": 2 } } }]
    });
    assert_eq!(units_from_document("a.rs", &document)[0].nesting, Some(2));

    let document = serde_json::json!({
        "spaces": [{ "name": "f", "kind": "function", "metrics": {} }]
    });
    assert_eq!(units_from_document("a.rs", &document)[0].nesting, None);
}

#[test]
fn a_bare_array_document_is_walked_too() {
    let document = serde_json::json!([
        { "spaces": [{ "name": "f", "kind": "function", "metrics": {} }] }
    ]);

    assert_eq!(units_from_document("a.rs", &document).len(), 1);
}

#[test]
fn function_metrics_are_read_from_the_tool() {
    let repo = MiniRepo::build(None);
    let document = serde_json::json!({
        "spaces": [{ "name": "parse", "kind": "function", "metrics": { "loc": { "sloc": 9 }, "mi": { "mi_original": 40.0 } } }]
    });
    let runner = FakeRunner::with(&[("rust-code-analysis-cli", 0, &document.to_string())]);
    let mut errors = Vec::new();

    let units = analysis_units(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "cargo",
        &mut errors,
    );

    assert_eq!(units.len(), 1);
    assert_eq!(units[0].name, "parse");
    assert!(errors.is_empty());
}

#[test]
fn a_file_the_tool_cannot_read_is_an_error() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("rust-code-analysis-cli", 1, "")]);
    let mut errors = Vec::new();

    let units = analysis_units(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "cargo",
        &mut errors,
    );

    assert!(units.is_empty());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("could not read it"));
}

#[test]
fn unparsable_json_is_an_error() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("rust-code-analysis-cli", 0, "{oops")]);
    let mut errors = Vec::new();

    analysis_units(
        &runner,
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &["src/foo.rs".to_string()],
        "cargo",
        &mut errors,
    );

    assert!(errors.iter().any(|error| error.contains("no usable json")));
}
