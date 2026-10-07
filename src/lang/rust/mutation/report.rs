//! The report a cargo-mutants run leaves behind: the counts and mutant names in
//! its `mutants.out/outcomes.json`, and the exclusion list an `--iterate` pass
//! read at its start.

use crate::lang::MutationReport;
use serde::Deserialize;
use std::path::Path;

/// The directory cargo-mutants writes into, under the `--output` parent.
const OUT_DIR: &str = "mutants.out";
const OUTCOMES: &str = "outcomes.json";
/// The mutants an `--iterate` pass excluded — its earlier passes' kills.
const PREVIOUSLY_CAUGHT: &str = "previously_caught.txt";

const SUCCESS: &str = "Success";
const MISSED: &str = "MissedMutant";
const TIMED_OUT: &str = "Timeout";

/// The report a finished run wrote into its output parent directory; `None`
/// when it wrote none, it cannot be read, or the run did not finish.
pub fn read(output_dir: &Path) -> Option<MutationReport> {
    let out = output_dir.join(OUT_DIR);
    let text = std::fs::read_to_string(out.join(OUTCOMES)).ok()?;
    let lab: LabReport = serde_json::from_str(&text).ok()?;
    // `end_time` is written when the lab finishes: without it the counts are a
    // mid-run snapshot, and no verdict can be read from them.
    lab.end_time.as_ref()?;

    let mut report = MutationReport {
        total: lab.total_mutants,
        caught: lab.caught,
        missed: lab.missed,
        unviable: lab.unviable,
        timeout: lab.timeout,
        skipped: lines_in(&out.join(PREVIOUSLY_CAUGHT)),
        ..MutationReport::default()
    };
    for outcome in &lab.outcomes {
        match (&outcome.scenario, outcome.summary.as_str()) {
            (Scenario::Baseline, summary) if summary != SUCCESS => {
                report.baseline_failure = Some(summary.to_string());
            }
            (Scenario::Mutant(mutant), MISSED) => report.survivors.push(mutant.name.clone()),
            (Scenario::Mutant(mutant), TIMED_OUT) => report.timed_out.push(mutant.name.clone()),
            _ => {}
        }
    }
    Some(report)
}

/// The lines of a list file the run may not have written at all.
fn lines_in(path: &Path) -> i64 {
    std::fs::read_to_string(path)
        .map(|text| text.lines().count() as i64)
        .unwrap_or_default()
}

/// The lab outcome cargo-mutants serializes: overall counters, the per-scenario
/// outcomes, and the end time a finished run carries.
#[derive(Deserialize)]
struct LabReport {
    total_mutants: i64,
    caught: i64,
    missed: i64,
    unviable: i64,
    timeout: i64,
    end_time: Option<String>,
    outcomes: Vec<ScenarioReport>,
}

#[derive(Deserialize)]
struct ScenarioReport {
    scenario: Scenario,
    summary: String,
}

#[derive(Deserialize)]
enum Scenario {
    Baseline,
    Mutant(MutantReport),
}

#[derive(Deserialize)]
struct MutantReport {
    name: String,
}

#[cfg(test)]
mod tests;
