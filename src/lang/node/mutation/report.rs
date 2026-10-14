//! The report a stryker run leaves behind: `mutation.json`, as the json
//! reporter writes it.

use crate::lang::MutationReport;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The report file the generated config points the json reporter at.
pub const REPORT_FILE: &str = "mutation.json";

/// Where the run's report lands: inside the run's own output directory.
pub fn report_path(output_dir: &Path) -> PathBuf {
    output_dir.join(REPORT_FILE)
}

const KILLED: &str = "Killed";
const SURVIVED: &str = "Survived";
const NO_COVERAGE: &str = "NoCoverage";
const TIMEOUT: &str = "Timeout";
const COMPILE_ERROR: &str = "CompileError";
/// Excluded by the repo's own `ignore` config: out of the run's total.
const IGNORED: &str = "Ignored";

/// The report a finished run wrote into its output directory; `None` when it
/// wrote none, or it cannot be read.
pub fn read(output_dir: &Path) -> Option<MutationReport> {
    let text = std::fs::read_to_string(report_path(output_dir)).ok()?;
    let report: StrykerReport = serde_json::from_str(&text).ok()?;
    Some(report.into_mutation_report())
}

/// The report stryker serializes: one entry per mutated file.
#[derive(Deserialize)]
struct StrykerReport {
    files: BTreeMap<String, FileReport>,
}

#[derive(Deserialize)]
struct FileReport {
    mutants: Vec<Mutant>,
}

#[derive(Deserialize)]
struct Mutant {
    status: String,
    #[serde(default, rename = "mutatorName")]
    mutator_name: String,
    #[serde(default)]
    replacement: String,
    location: Option<Location>,
}

#[derive(Deserialize)]
struct Location {
    start: Position,
}

#[derive(Deserialize)]
struct Position {
    line: i64,
    column: i64,
}

impl Mutant {
    /// `file:line:col mutator -> replacement`, as the evidence lists it.
    fn name(&self, file: &str) -> String {
        let where_ = match &self.location {
            Some(location) => format!("{file}:{}:{}", location.start.line, location.start.column),
            None => file.to_string(),
        };
        format!("{where_} {} -> {}", self.mutator_name, self.replacement)
    }
}

impl StrykerReport {
    /// The counts mido judges. Every status but `Ignored` counts into the
    /// total; a status the runner cannot classify keeps its mutant in the
    /// total without a verdict, which the gate reports as INCOMPLETE — never
    /// as a kill and never as a silent drop.
    fn into_mutation_report(self) -> MutationReport {
        let mut report = MutationReport::default();

        for (file, info) in &self.files {
            for mutant in &info.mutants {
                if mutant.status == IGNORED {
                    continue;
                }
                match mutant.status.as_str() {
                    KILLED => report.caught += 1,
                    SURVIVED | NO_COVERAGE => {
                        report.missed += 1;
                        report.survivors.push(mutant.name(file));
                    }
                    TIMEOUT => {
                        report.timeout += 1;
                        report.timed_out.push(mutant.name(file));
                    }
                    COMPILE_ERROR => report.unviable += 1,
                    _ => {}
                }
                report.total += 1;
            }
        }
        report
    }
}

#[cfg(test)]
mod tests;
