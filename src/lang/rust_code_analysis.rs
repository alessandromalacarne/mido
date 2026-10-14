//! rust-code-analysis: one metrics document per file, read into the `Unit`s the
//! rust module's size and analysis gates judge.

use crate::metrics::Unit;
use crate::process::{self, Runner};
use crate::targets::Target;
use serde_json::Value as Json;
use std::path::Path;

/// The metrics tool this runner drives.
pub const TOOL: &str = "rust-code-analysis";

pub fn analysis_units(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    probe_tool: &str,
    on_error: &mut Vec<String>,
) -> Vec<Unit> {
    let mut units = Vec::new();

    for path in files {
        match measure_one(runner, repo, target, path, probe_tool) {
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
    probe_tool: &str,
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
    let result = process::dev(runner, probe_tool, &target.dir(repo), &args, None);
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
fn units_from_document(path: &str, node: &Json) -> Vec<Unit> {
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

fn unit_from_space(path: &str, space: &Json) -> Unit {
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
        mi: family_float(&metrics, "mi", "mi_original"),
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
mod tests;
