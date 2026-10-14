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
fn a_function_a_rule_reported_unnamed_keeps_its_reading() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "max-lines-per-function", "message": "Arrow function has too many lines (9). Maximum allowed is 0.", "line": 4, "column": 20 },
                { "ruleId": "complexity", "message": "Arrow function has a complexity of 4. Maximum allowed is 0.", "line": 4, "column": 20 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert_eq!(measurements.units.len(), 1);
    assert_eq!(measurements.units[0].name, "arrow function");
    assert_eq!(measurements.units[0].sloc, 9);
    assert_eq!(measurements.units[0].cyclomatic, 4);
}

#[test]
fn a_function_one_rule_did_not_report_keeps_that_reading_at_its_floor() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "complexity", "message": "Function 'f' has a complexity of 9. Maximum allowed is 0.", "line": 2, "column": 1 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    let unit = &measurements.units[0];
    assert_eq!(unit.cyclomatic, 9);
    assert_eq!(unit.sloc, 0);
    assert_eq!(unit.cognitive, 0);
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
fn the_projects_own_lint_findings_are_left_to_the_syntax_gate() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "no-unused-vars", "message": "'x' is defined but never used.", "line": 1, "column": 7 },
                { "ruleId": "@typescript-eslint/no-explicit-any", "message": "Unexpected any.", "line": 2, "column": 9 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert!(measurements.units.is_empty());
    assert!(measurements.errors.is_empty(), "{:?}", measurements.errors);
}

#[test]
fn a_file_eslint_could_not_parse_is_a_measurement_error() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": null, "fatal": true, "severity": 2, "message": "Parsing error: ':' expected.", "line": 2, "column": 9 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert_eq!(
        measurements.errors,
        vec!["src/a.js: Parsing error: ':' expected.".to_string()]
    );
}

#[test]
fn a_file_the_config_leaves_out_is_a_measurement_error() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": null, "fatal": false, "severity": 1, "message": "File ignored because no matching configuration was supplied." }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert!(measurements.errors[0].contains("no matching configuration"));
}

#[test]
fn a_file_with_no_report_at_all_is_a_measurement_error() {
    let asked = vec!["src/a.js".to_string(), "src/b.js".to_string()];
    let document = serde_json::json!([{ "filePath": "/repo/src/a.js", "messages": [] }]);

    let measurements = document::readings(&asked, &document);

    assert_eq!(
        measurements.errors,
        vec!["src/b.js: eslint returned no report".to_string()]
    );
}

#[test]
fn a_message_mido_cannot_read_is_an_error_not_a_zero() {
    let document = serde_json::json!([
        {
            "filePath": "/repo/src/a.js",
            "messages": [
                { "ruleId": "complexity", "message": "Function 'f' is far too complicated for words.", "line": 1, "column": 1 }
            ]
        }
    ]);

    let measurements = document::readings(&files(), &document);

    assert_eq!(measurements.errors.len(), 1);
    assert!(measurements.errors[0].contains("`complexity`"));
    assert!(measurements.errors[0].contains("in a shape mido cannot read"));
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
