use super::*;
use crate::test_support::{FakeRunner, MiniJsRepo};

fn files() -> Vec<String> {
    vec!["src/a.js".to_string()]
}

fn repo() -> MiniJsRepo {
    MiniJsRepo::build(None)
}

fn target() -> Target {
    Target::workspace_target("package.json")
}

#[test]
fn a_document_answers_with_lines_nesting_and_units() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "max-lines", "message": "File has too many lines (120). Maximum allowed is 0." },
                { "ruleId": "max-lines-per-function", "message": "Function 'classify' has too many lines (17). Maximum allowed is 0.", "line": 3, "column": 1 },
                { "ruleId": "complexity", "message": "Function 'classify' has a complexity of 7. Maximum allowed is 0.", "line": 3, "column": 1 },
                { "ruleId": "sonarjs/cognitive-complexity", "message": "Refactor this function to reduce its Cognitive Complexity from 12 to the 0 allowed.", "line": 3, "column": 10 },
                { "ruleId": "max-depth", "message": "Blocks are nested too deeply (2). Maximum allowed is 0.", "line": 5, "column": 3 },
                { "ruleId": "max-depth", "message": "Blocks are nested too deeply (4). Maximum allowed is 0.", "line": 6, "column": 5 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert_eq!(measurements.files, files());
    assert_eq!(measurements.lines["src/a.js"], 120);
    assert_eq!(measurements.nesting["src/a.js"], 4);
    assert!(measurements.errors.is_empty(), "{:?}", measurements.errors);
    assert_eq!(measurements.units.len(), 1);
    let unit = &measurements.units[0];
    assert_eq!(unit.path, "src/a.js");
    assert_eq!(unit.name, "classify");
    assert_eq!(unit.sloc, 17);
    assert_eq!(unit.cyclomatic, 7);
    assert_eq!(unit.cognitive, 12);
    assert_eq!(unit.nesting, None);
    assert_eq!(unit.mi, None);
}

#[test]
fn a_cognitive_reading_lands_on_the_function_it_was_reported_inside() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "complexity", "message": "Arrow function has a complexity of 2. Maximum allowed is 0.", "line": 7, "column": 8 },
                { "ruleId": "complexity", "message": "Arrow function has a complexity of 3. Maximum allowed is 0.", "line": 7, "column": 24 },
                { "ruleId": "sonarjs/cognitive-complexity", "message": "Refactor this function to reduce its Cognitive Complexity from 5 to the 0 allowed.", "line": 7, "column": 25 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert_eq!(measurements.units.len(), 2);
    assert_eq!(measurements.units[0].cognitive, 0);
    assert_eq!(measurements.units[1].cognitive, 5);
}

#[test]
fn the_pass_injects_the_metric_rules_and_reads_them_back() {
    let repo = repo();
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "max-lines", "message": "File has too many lines (42). Maximum allowed is 0." },
                { "ruleId": "complexity", "message": "Function 'f' has a complexity of 3. Maximum allowed is 0.", "line": 1, "column": 1 }
            ]
        }
    ]);
    let runner = FakeRunner::with(&[("eslint", 0, &document.to_string())]).tool("node");

    let measurements = measure(&runner, &repo.root, &target(), &files(), "node");

    assert!(runner.called_with("npx --no-install eslint --format json --rule"));
    assert!(runner.called_with("max-lines-per-function"));
    assert!(runner.called_with("sonarjs/cognitive-complexity"));
    assert!(runner.called_with("src/a.js"));
    assert_eq!(measurements.lines["src/a.js"], 42);
    assert_eq!(measurements.units.len(), 1);
}

#[test]
fn output_that_is_not_json_is_a_measurement_error() {
    let repo = repo();
    let runner = FakeRunner::with(&[("eslint", 2, "not a document at all")]).tool("node");

    let measurements = measure(&runner, &repo.root, &target(), &files(), "node");

    assert_eq!(measurements.files, files());
    assert!(measurements.units.is_empty());
    assert_eq!(measurements.errors.len(), 1);
    assert!(measurements.errors[0].contains("eslint printed no usable json (exit 2)"));
    assert!(measurements.errors[0].contains("not a document at all"));
}

#[test]
fn eslint_complaining_on_stderr_says_what_is_wrong() {
    let repo = repo();
    let mut runner = FakeRunner::default().tool("node");
    runner.responses.push((
        "eslint".to_string(),
        Outcome::new(
            2,
            "",
            "\nOops! Something went wrong! :(\n\nESLint: 10.12.0\n\nA configuration object specifies rule \"sonarjs/cognitive-complexity\", but could not find plugin \"sonarjs\".\n",
        ),
    ));

    let measurements = measure(&runner, &repo.root, &target(), &files(), "node");

    assert_eq!(measurements.errors.len(), 1);
    assert!(measurements.errors[0].contains("could not find plugin \"sonarjs\""));
}

#[test]
fn nothing_to_measure_runs_nothing() {
    let repo = repo();
    let runner = FakeRunner::default().tool("node");

    let measurements = measure(&runner, &repo.root, &target(), &[], "node");

    assert!(measurements.errors.is_empty());
    assert!(measurements.units.is_empty());
    assert!(!runner.called_with("eslint"));
}
