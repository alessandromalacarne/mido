//! The rust module's own measurement pass: tokei counts the code lines, and
//! rust-code-analysis reads every function's metrics.

use crate::lang::{rust_code_analysis, tokei};
use crate::metrics::Measurements;
use crate::process::Runner;
use crate::targets::Target;
use std::path::Path;

pub fn measure(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    probe_tool: &str,
) -> Measurements {
    let mut measurements = Measurements {
        files: files.to_vec(),
        ..Measurements::default()
    };
    if files.is_empty() {
        return measurements;
    }

    measurements.lines = tokei::code_lines(
        runner,
        repo,
        target,
        files,
        probe_tool,
        &mut measurements.errors,
    );
    measurements.units = rust_code_analysis::analysis_units(
        runner,
        repo,
        target,
        files,
        probe_tool,
        &mut measurements.errors,
    );
    measurements
}

#[cfg(test)]
mod tests;
