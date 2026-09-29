//! Readings the gates judge: rust-code-analysis documents and lcov reports.

use crate::targets::Target;
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::path::Path;

/// One measured function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub path: String,
    pub name: String,
    pub sloc: i64,
    pub cyclomatic: i64,
    pub cognitive: i64,
    pub nesting: Option<i64>,
    pub mi: f64,
}

impl Unit {
    pub fn new(path: &str, name: &str) -> Self {
        Self {
            path: path.to_string(),
            name: name.to_string(),
            sloc: 0,
            cyclomatic: 0,
            cognitive: 0,
            nesting: None,
            mi: 100.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LcovStat {
    pub lines_found: i64,
    pub lines_hit: i64,
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

pub fn lcov_files(path: &Path) -> BTreeMap<String, LcovStat> {
    let mut files = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return files;
    };

    let mut current: Option<String> = None;
    for line in text.lines() {
        apply_lcov_line(line, &mut current, &mut files);
    }
    files
}

/// One `SF:`/`LF:`/`LH:` line: a new source file, or a count for the current one.
fn apply_lcov_line(
    line: &str,
    current: &mut Option<String>,
    files: &mut BTreeMap<String, LcovStat>,
) {
    if let Some(source) = line.strip_prefix("SF:") {
        files.insert(source.to_string(), LcovStat::default());
        *current = Some(source.to_string());
    } else if let Some(found) = line.strip_prefix("LF:") {
        if let Some(stat) = stat_for(current, files) {
            stat.lines_found = found.trim().parse().unwrap_or_default();
        }
    } else if let Some(hit) = line.strip_prefix("LH:") {
        if let Some(stat) = stat_for(current, files) {
            stat.lines_hit = hit.trim().parse().unwrap_or_default();
        }
    }
}

fn stat_for<'a>(
    current: &Option<String>,
    files: &'a mut BTreeMap<String, LcovStat>,
) -> Option<&'a mut LcovStat> {
    files.get_mut(current.as_ref()?)
}

pub fn percent(hit: i64, found: i64) -> f64 {
    if found == 0 {
        0.0
    } else {
        100.0 * hit as f64 / found as f64
    }
}

pub fn relative_to(path: &str, target: &Target) -> String {
    let marker = if target.path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", target.path)
    };
    match path.find(&marker) {
        Some(index) => path[index + marker.len()..].to_string(),
        None => path.to_string(),
    }
}

pub fn touches(diagnostic_path: &str, changed: &[String]) -> bool {
    changed
        .iter()
        .any(|path| diagnostic_path.ends_with(path.as_str()) || path.ends_with(diagnostic_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target::crate_target("frontend", false)
    }

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
    fn lcov_counts_each_source_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let report = directory.path().join("lcov.info");
        std::fs::write(
            &report,
            "SF:/repo/lib/src/foo.rs\nLF:10\nLH:2\nend_of_record\nSF:/repo/lib/src/bar.rs\nLF:4\nLH:4\nend_of_record\n",
        )
        .expect("report");

        let files = lcov_files(&report);

        assert_eq!(files.len(), 2);
        assert_eq!(
            files["/repo/lib/src/foo.rs"],
            LcovStat {
                lines_found: 10,
                lines_hit: 2
            }
        );
        assert_eq!(
            files["/repo/lib/src/bar.rs"],
            LcovStat {
                lines_found: 4,
                lines_hit: 4
            }
        );
    }

    #[test]
    fn a_missing_report_measures_nothing() {
        assert!(lcov_files(Path::new("/nonexistent/lcov.info")).is_empty());
    }

    #[test]
    fn percent_handles_the_empty_denominator() {
        assert_eq!(percent(0, 0), 0.0);
        assert_eq!(percent(2, 10), 20.0);
    }

    #[test]
    fn paths_are_shown_relative_to_the_target() {
        assert_eq!(
            relative_to("/repo/frontend/src/main.rs", &target()),
            "src/main.rs"
        );
        assert_eq!(
            relative_to("/repo/frontend/src/main.rs", &Target::workspace_target()),
            "repo/frontend/src/main.rs"
        );
        assert_eq!(relative_to("src/main.rs", &target()), "src/main.rs");
    }

    #[test]
    fn a_diagnostic_touches_a_changed_file_in_either_direction() {
        let changed = vec!["src/main.rs".to_string()];

        assert!(touches("src/main.rs", &changed));
        assert!(touches("/abs/path/src/main.rs", &changed));
        assert!(!touches("src/other.rs", &changed));
    }
}
