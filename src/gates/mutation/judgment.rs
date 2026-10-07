//! Judging the mutation passes: run each one, read the report it wrote, and
//! turn the counts into the gate's verdict.

use super::MutationPass;
use crate::gate::Gate;
use crate::gates::GateRun;
use crate::lang::{Lang, MutationReport};
use crate::metrics::percent;
use crate::process::last_lines;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use std::path::Path;

/// One pass's evidence: what it ran, and the report it wrote.
pub(super) struct PassReport {
    command: Vec<String>,
    report: MutationReport,
}

/// Why a pass could not be judged: the summary to report, and the tool's own
/// tail as the evidence.
pub(super) struct Unjudged {
    pub(super) summary: String,
    pub(super) details: Vec<String>,
}

/// Run the passes in order, reading each pass's report before the next one
/// rotates it away. Each pass gets the whole mutation timeout, as every command
/// of the tests gate does.
pub(super) fn mutation_reports(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    passes: &[MutationPass],
    timeout: i64,
    output: &Path,
) -> Result<Vec<PassReport>, Unjudged> {
    let (lang, repo, target) = (run.lang, run.repo, run.target);
    let mut reports: Vec<PassReport> = Vec::with_capacity(passes.len());

    for pass in passes {
        let outcome = process::dev(
            runner,
            lang.env_tool(),
            &target.dir(repo),
            &pass.args,
            Some(timeout.max(0) as u64),
        );
        let text = outcome.combined();

        if outcome.code == process::TIMEOUT_EXIT {
            return Err(Unjudged {
                summary: format!("timed out after {timeout}s"),
                details: last_lines(&text, 10),
            });
        }

        let Some(report) = lang.mutation_report(output) else {
            return Err(Unjudged {
                summary: no_report(lang),
                details: last_lines(&text, 10),
            });
        };

        reports.push(PassReport {
            command: pass.command.clone(),
            report,
        });
    }

    Ok(reports)
}

/// The verdict the passes earn. The last pass decides, because `--iterate` makes
/// the mutants the earlier passes caught show up as skipped — and skipped counts
/// as killed.
pub(super) fn judge_mutation(reports: &[PassReport], contract: &str, minimum: f64) -> GateResult {
    let last = reports.last().expect("a passing run always reports");
    let report = &last.report;

    if let Some(failure) = &report.baseline_failure {
        return incomplete(
            format!("the unmutated baseline did not pass ({failure})"),
            Vec::new(),
            contract,
        );
    }

    // A mutant with neither a kill nor a miss is no verdict at all: the tool ran
    // out of time on it, or could not classify it. Never a pass.
    let unresolved = report.total - report.caught - report.missed - report.unviable;
    if unresolved > 0 {
        let mut details: Vec<String> = report
            .timed_out
            .iter()
            .map(|name| format!("TIMEOUT  {name}"))
            .collect();
        let unclassified = unresolved - report.timed_out.len() as i64;
        if unclassified > 0 {
            details.push(format!("{unclassified} mutants did not report an outcome"));
        }
        return incomplete(
            format!(
                "{unresolved} of {} mutants produced no verdict",
                report.total
            ),
            details,
            contract,
        );
    }

    let killed = report.caught + report.skipped;
    let rate = percent(killed, killed + report.missed);
    let status = if rate < minimum { FAIL } else { PASS };
    let mut details = mutation_details(report);
    details.extend(pass_lines(reports));

    GateResult::new(
        "mutation",
        status,
        format!("{rate:.1}% killed (min {minimum})"),
        details,
    )
    .contract(contract)
    .fixes(Gate::Mutation.fix_hints().iter().copied())
}

/// The summary an INCOMPLETE pass reports: the tool wrote nothing to judge.
fn no_report(lang: Lang) -> String {
    format!("{} produced no report", lang.mutation_tool())
}

/// One line per pass — which suite ran, and what it reported. A single-pass run
/// keeps the shorter evidence.
fn pass_lines(reports: &[PassReport]) -> Vec<String> {
    if reports.len() < 2 {
        return Vec::new();
    }

    reports
        .iter()
        .map(|pass| {
            format!(
                "pass `{}`: {} caught, {} missed, {} skipped",
                pass.command.join(" "),
                pass.report.caught,
                pass.report.missed,
                pass.report.skipped
            )
        })
        .collect()
}

/// The counts line plus every survivor the deciding pass listed. Mutants an
/// `--iterate` pass excluded were caught (or unviable) by this run's earlier
/// passes and count as killed.
fn mutation_details(report: &MutationReport) -> Vec<String> {
    let killed = report.caught + report.skipped;
    let total = report.total + report.skipped;
    let rate = percent(killed, killed + report.missed);
    let skipped_note = if report.skipped > 0 {
        format!(
            ", {} previously caught or unviable (skipped)",
            report.skipped
        )
    } else {
        String::new()
    };
    let mut details = vec![format!(
        "{total} mutants: {} caught, {} missed, {} unviable{skipped_note} -> {rate:.1}% killed",
        report.caught, report.missed, report.unviable
    )];
    details.extend(
        report
            .survivors
            .iter()
            .map(|name| format!("MISSED  {name}")),
    );
    details
}

/// An INCOMPLETE verdict carrying the tool's own tail as evidence.
pub(super) fn incomplete(summary: String, details: Vec<String>, contract: &str) -> GateResult {
    GateResult::new("mutation", INCOMPLETE, summary, details)
        .contract(contract)
        .fixes(Gate::Mutation.fix_hints().iter().copied())
}
