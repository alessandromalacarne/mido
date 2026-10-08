//! The run's panels: the opening banner and the failure report.

use super::panel::{field, panel, short, status_glyph};
use super::{blocked_verdict, GateResult, INCOMPLETE, MAX_BANNER_FILES};
use crate::gate::GATES;
use crate::style::Style;
use crate::targets::Target;

#[derive(Debug, Clone, Default)]
pub struct BannerContext<'a> {
    pub revision: &'a str,
    pub dirty: &'a str,
    pub files: &'a [String],
    pub source_label: &'a str,
    pub source_count: usize,
}

pub fn render_banner(target: &Target, context: &BannerContext<'_>, style: Style) -> String {
    let mut lines: Vec<String> = panel("mido", &banner_rows(target, context), style)
        .lines()
        .map(str::to_string)
        .collect();
    lines.extend(files_listing(context, style));
    lines.join("\n")
}

fn banner_rows(target: &Target, context: &BannerContext<'_>) -> Vec<String> {
    vec![
        field("target", &target.label()),
        field("revision", short(or_unknown(context.revision))),
        field("dirty", short(or_unknown(context.dirty))),
        field(
            "measured",
            &format!(
                "{} files ({} {})",
                context.files.len(),
                context.source_count,
                context.source_label
            ),
        ),
    ]
}

/// The measured files under the panel — capped, with the remainder counted.
fn files_listing(context: &BannerContext<'_>, style: Style) -> Vec<String> {
    let mut lines: Vec<String> = context
        .files
        .iter()
        .take(MAX_BANNER_FILES)
        .map(|path| style.dim(&format!("  {path}")))
        .collect();
    if context.files.len() > MAX_BANNER_FILES {
        lines.push(style.dim(&format!(
            "  … and {} more",
            context.files.len() - MAX_BANNER_FILES
        )));
    }
    lines
}

fn or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "unknown"
    } else {
        value
    }
}

#[derive(Debug, Clone, Default)]
pub struct FailureContext<'a> {
    pub target: &'a str,
    pub revision: &'a str,
    pub dirty: &'a str,
    pub attempts: Option<i64>,
}

pub fn render_failure(
    results: &[GateResult],
    context: &FailureContext<'_>,
    style: Style,
) -> String {
    let blocked: Vec<&GateResult> = results.iter().filter(|result| !result.passed()).collect();
    let mut lines = failure_header(&blocked, results.len(), context, style);

    for result in &blocked {
        lines.extend(render_gate_block(result, context, style));
    }

    if blocked.iter().any(|result| result.status == INCOMPLETE) {
        lines.push(style.dim("note: INCOMPLETE means the gate did not run to a verdict — a missing tool, an unanalyzable file or a"));
        lines.push(style.dim("      timeout is never a pass. Fix the tooling (nix develop) and re-run before handing this off."));
        lines.push(String::new());
    }

    format!("{}\n", lines.join("\n").trim_end())
}

fn failure_header(
    blocked: &[&GateResult],
    total: usize,
    context: &FailureContext<'_>,
    style: Style,
) -> Vec<String> {
    let rows = vec![
        style.fail(&blocked_verdict(blocked)),
        field("target", context.target),
        field("revision", short(or_unknown(context.revision))),
        field("dirty", short(or_unknown(context.dirty))),
        field("failing", &format!("{} of {total} gates", blocked.len())),
    ];

    let mut lines: Vec<String> = panel("failure", &rows, style)
        .lines()
        .map(str::to_string)
        .collect();
    lines.push(String::new());
    lines
}

/// A titled list — `evidence`, `fix` — with the summary echo and empty lines
/// left out.
fn render_list(title: &str, items: &[String], skip: &str) -> Vec<String> {
    let kept: Vec<&str> = items
        .iter()
        .map(String::as_str)
        .filter(|item| !item.is_empty() && *item != skip)
        .collect();
    if kept.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![format!("      {title}")];
    lines.extend(kept.into_iter().map(|item| format!("        - {item}")));
    lines
}

/// One blocked gate: position in the ladder, contract, evidence, fix.
fn render_gate_block(
    result: &GateResult,
    context: &FailureContext<'_>,
    style: Style,
) -> Vec<String> {
    let order = GATES
        .iter()
        .position(|gate| gate.name() == result.name)
        .map(|index| index + 1)
        .unwrap_or(0);
    let head = format!(
        "{} {} — {}",
        status_glyph(&result.status),
        result.name,
        result.status
    );
    let mut lines = vec![
        format!(
            "  [{order}/{}] {}",
            GATES.len(),
            style.paint_status(&result.status, &head)
        ),
        format!("      summary   {}", result.summary),
    ];

    if !result.contract.is_empty() {
        lines.push(format!("      contract  {}", result.contract));
    }
    lines.extend(render_list("evidence", &result.details, &result.summary));
    lines.extend(render_list("fix", &result.fixes, ""));
    if let Some(attempts) = context.attempts {
        lines.push(format!(
            "      attempts  max {attempts} distinct hypotheses per gate (`.mido.toml` [failure])"
        ));
    }
    lines.push(String::new());
    lines
}

#[cfg(test)]
mod tests;
