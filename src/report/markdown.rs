//! The markdown report the next step reads.

use super::{verdict, GateResult};
use crate::targets::Target;

pub struct ReportContext<'a> {
    pub target: &'a Target,
    pub base: &'a str,
    pub revision: &'a str,
    pub dirty: &'a str,
    pub changed: &'a [String],
    pub source_label: &'a str,
    pub source_count: usize,
}

pub fn render_report_markdown(results: &[GateResult], context: &ReportContext<'_>) -> String {
    let mut lines = report_header(context, &verdict(results));
    lines.extend(summary_table(results));
    lines.push(String::new());

    for result in results {
        lines.extend(gate_details(result));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn report_header(context: &ReportContext<'_>, verdict_line: &str) -> Vec<String> {
    let mut lines = vec![
        format!("# Guardrails report — {}", context.target.name),
        String::new(),
        format!("VERDICT: {verdict_line}"),
        format!("revision: `{}`", context.revision),
        format!("dirty state hash: `{}`", context.dirty),
        if context.base.is_empty() {
            "scope: `explicit paths`".to_string()
        } else {
            format!("base: `{}`", context.base)
        },
        format!("target: `{}`", context.target.label()),
    ];
    lines.push(format!(
        "changed files: {} ({} {})",
        context.changed.len(),
        context.source_count,
        context.source_label
    ));
    lines.push(String::new());
    lines
}

fn summary_table(results: &[GateResult]) -> Vec<String> {
    let mut lines = vec![
        "| Gate | Status | Evidence |".to_string(),
        "| --- | --- | --- |".to_string(),
    ];
    lines.extend(results.iter().map(|result| {
        format!(
            "| {} | {} | {} |",
            result.name, result.status, result.summary
        )
    }));
    lines
}

fn gate_details(result: &GateResult) -> Vec<String> {
    let mut lines = vec![
        format!("### {}", result.name),
        String::new(),
        format!("contract: {}", result.contract),
        String::new(),
    ];
    lines.extend(result.details.iter().map(|detail| format!("- {detail}")));
    if !result.fixes.is_empty() {
        lines.push(String::new());
        lines.push("fix:".to_string());
        lines.extend(result.fixes.iter().map(|fix| format!("- {fix}")));
    }
    lines
}

#[cfg(test)]
mod tests;
