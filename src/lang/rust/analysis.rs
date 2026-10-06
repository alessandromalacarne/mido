//! rust-code-analysis: running the CLI, and reading its document into `Unit`s.

use crate::metrics::Unit;
use crate::process::{self, Runner};
use crate::targets::Target;
use serde_json::Value as Json;
use std::path::Path;

pub fn analysis_units(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    on_error: &mut Vec<String>,
) -> Vec<Unit> {
    let mut units = Vec::new();

    for path in files {
        match measure_one(runner, repo, target, path) {
            Ok(document) => units.extend(units_from_document(path, &document)),
            Err(message) => on_error.push(message),
        }
    }
    units
}

/// One file's metrics document, or the error line for the gate's evidence.
fn measure_one(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    path: &str,
) -> Result<Json, String> {
    let args: Vec<String> = [
        "rust-code-analysis-cli",
        "-m",
        "--pr",
        "-O",
        "json",
        "-p",
        path,
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect();
    let result = process::dev(runner, super::ENV_TOOL, &target.dir(repo), &args, None);
    if !result.ok() || result.stdout.trim().is_empty() {
        return Err(format!(
            "{path}: rust-code-analysis could not read it (exit {})",
            result.code
        ));
    }

    serde_json::from_str(&result.stdout)
        .map_err(|error| format!("{path}: rust-code-analysis printed no usable json ({error})"))
}

/// Every function/metric in one rust-code-analysis document, at any depth.
pub fn units_from_document(path: &str, node: &Json) -> Vec<Unit> {
    match node {
        Json::Array(items) => items
            .iter()
            .flat_map(|item| units_from_document(path, item))
            .collect(),
        Json::Object(space) => {
            let mut units = Vec::new();
            for child in space
                .get("spaces")
                .and_then(Json::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                if matches!(
                    child.get("kind").and_then(Json::as_str),
                    Some("function" | "method")
                ) {
                    units.push(unit_from_space(path, child));
                }
                units.extend(units_from_document(path, child));
            }
            units
        }
        _ => Vec::new(),
    }
}

pub fn unit_from_space(path: &str, space: &Json) -> Unit {
    let metrics = space.get("metrics").cloned().unwrap_or(Json::Null);

    Unit {
        path: path.to_string(),
        name: space
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        sloc: family_value(&metrics, "loc", "sloc").unwrap_or_default(),
        cyclomatic: family_value(&metrics, "cyclomatic", "sum").unwrap_or_default(),
        cognitive: family_value(&metrics, "cognitive", "sum").unwrap_or_default(),
        nesting: nesting_of(&metrics),
        mi: family_float(&metrics, "mi", "mi_original").unwrap_or(100.0),
    }
}

fn family<'a>(metrics: &'a Json, name: &str) -> Option<&'a Json> {
    metrics.get(name).filter(|value| value.is_object())
}

fn family_value(metrics: &Json, family_name: &str, key: &str) -> Option<i64> {
    number(family(metrics, family_name)?.get(key)?)
}

fn family_float(metrics: &Json, family_name: &str, key: &str) -> Option<f64> {
    let value = family(metrics, family_name)?.get(key)?;
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|number| number as f64))
}

fn number(value: &Json) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|number| number as i64))
}

fn nesting_of(metrics: &Json) -> Option<i64> {
    ["nested_control_flow", "nesting"]
        .iter()
        .find_map(|key| family_value(metrics, key, "sum"))
}

#[cfg(test)]
mod tests {
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
        assert!((units[0].mi - 55.5).abs() < f64::EPSILON);
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
            &Target::workspace_target(crate::lang::rust::MANIFEST),
            &["src/foo.rs".to_string()],
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
            &Target::workspace_target(crate::lang::rust::MANIFEST),
            &["src/foo.rs".to_string()],
            &mut errors,
        );

        assert!(units.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("could not read it"));
    }

    #[test]
    fn a_failing_tool_run_is_an_error_even_with_partial_output() {
        let repo = MiniRepo::build(None);
        let runner = FakeRunner::with(&[("rust-code-analysis-cli", 1, "{}")]);
        let mut errors = Vec::new();

        let units = analysis_units(
            &runner,
            &repo.root,
            &Target::workspace_target(crate::lang::rust::MANIFEST),
            &["src/foo.rs".to_string()],
            &mut errors,
        );

        assert!(units.is_empty());
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
            &Target::workspace_target(crate::lang::rust::MANIFEST),
            &["src/foo.rs".to_string()],
            &mut errors,
        );

        assert!(errors.iter().any(|error| error.contains("no usable json")));
    }
}
