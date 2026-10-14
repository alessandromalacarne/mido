//! eslint: the measurement tool the javascript and typescript modules drive.
//! One `eslint --format json` pass, with the metric rules injected at their
//! floor, answers for a target's files at once — the project's own eslint
//! config parses them, mido only asks what to measure.

pub mod document;

use crate::metrics::Measurements;
use crate::process::{self, Outcome, Runner};
use crate::targets::Target;
use serde_json::Value as Json;
use std::path::Path;

/// The tool these modules measure with.
pub const TOOL: &str = "eslint";

/// What one eslint pass answered for a target's files.
pub fn measure(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    probe_tool: &str,
) -> Measurements {
    let measurements = Measurements {
        files: files.to_vec(),
        ..Measurements::default()
    };
    if files.is_empty() {
        return measurements;
    }

    let result = process::dev(runner, probe_tool, &target.dir(repo), &argv(files), None);
    match document_of(&result) {
        Ok(document) => document::readings(files, &document),
        Err(error) => Measurements {
            errors: vec![error],
            ..measurements
        },
    }
}

/// The json document the pass printed, or the error line the gate reports
/// instead: eslint's own complaint says far more than a bare exit code.
fn document_of(result: &Outcome) -> Result<Json, String> {
    if let Ok(document) = serde_json::from_str::<Json>(&result.stdout) {
        return Ok(document);
    }

    let complaint = complaint(&result.stderr).or_else(|| complaint(&result.stdout));
    Err(match complaint {
        Some(line) => format!(
            "{TOOL} printed no usable json (exit {}): {line}",
            result.code
        ),
        None => format!("{TOOL} printed no usable json (exit {})", result.code),
    })
}

/// The tool's own complaint, without the banner eslint prints above it.
fn complaint(output: &str) -> Option<&str> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| !line.starts_with("Oops!") && !line.starts_with("ESLint:"))
}

/// The argv: the metric rules carry what to measure, at a floor that makes
/// every reading report itself.
fn argv(files: &[String]) -> Vec<String> {
    let mut args: Vec<String> = ["npx", "--no-install", TOOL, "--format", "json", "--rule"]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
    args.push(rules());
    args.extend(files.iter().cloned());
    args
}

fn rules() -> String {
    serde_json::json!({
        document::MAX_LINES: ["warn", { "max": 0, "skipBlankLines": true, "skipComments": true }],
        document::FUNCTION_LINES: ["warn", { "max": 0, "skipBlankLines": true, "skipComments": true }],
        document::COMPLEXITY: ["warn", { "max": 0 }],
        document::NESTING: ["warn", { "max": 0 }],
        document::COGNITIVE: ["warn", 0],
    })
    .to_string()
}

#[cfg(test)]
mod tests;
