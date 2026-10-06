//! tokei: the code lines per file the size gate judges.

use crate::process::{self, Runner};
use crate::targets::Target;
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::path::Path;

/// The size tool this module drives.
pub const TOOL: &str = "tokei";

/// Code lines per file, as tokei reports them; measurement failures land in
/// `on_error` and measure nothing — a missing reading is never a zero.
pub fn code_lines(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    tool: &str,
    on_error: &mut Vec<String>,
) -> BTreeMap<String, i64> {
    if files.is_empty() {
        return BTreeMap::new();
    }
    if tool != TOOL {
        on_error.push(format!(
            "size tool `{tool}` is not supported by this runner (only {TOOL})"
        ));
        return BTreeMap::new();
    }

    let args: Vec<String> = [TOOL, "--output", "json"]
        .iter()
        .map(|arg| arg.to_string())
        .chain(files.iter().cloned())
        .collect();
    let result = process::dev(runner, super::ENV_TOOL, &target.dir(repo), &args, None);
    if !result.ok() || result.stdout.trim().is_empty() {
        on_error.push(format!(
            "{TOOL} could not measure {} file(s) (exit {})",
            files.len(),
            result.code
        ));
        return BTreeMap::new();
    }

    let Ok(document) = serde_json::from_str::<Json>(&result.stdout) else {
        on_error.push(format!("{TOOL} printed no usable json"));
        return BTreeMap::new();
    };
    counts(&document)
}

/// `{language: {reports: [{name, stats: {code}}]}}`, minus the `Total` pseudo-language.
fn counts(document: &Json) -> BTreeMap<String, i64> {
    let mut counts = BTreeMap::new();
    let Some(languages) = document.as_object() else {
        return BTreeMap::new();
    };

    for (language, info) in languages {
        if language == "Total" {
            continue;
        }
        for report in info
            .get("reports")
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let Some(name) = report.get("name").and_then(Json::as_str) else {
                continue;
            };
            let code = report
                .get("stats")
                .and_then(|stats| stats.get("code"))
                .and_then(Json::as_i64)
                .unwrap_or_default();
            counts.insert(name.to_string(), code);
        }
    }
    counts
}

#[cfg(test)]
mod tests;
