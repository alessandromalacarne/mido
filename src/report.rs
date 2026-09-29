//! Verdicts and the reports the next step reads.

use crate::style::Style;
use crate::targets::Target;
use std::collections::BTreeSet;

pub const PASS: &str = "PASS";
pub const FAIL: &str = "FAIL";
pub const INCOMPLETE: &str = "INCOMPLETE";
pub const SKIPPED: &str = "SKIPPED";
pub const SHIP_READY: &str = "SHIP-READY";

pub const GATES: [&str; 6] = [
    "syntax", "size", "analysis", "tests", "coverage", "mutation",
];
pub const MAX_BANNER_FILES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResult {
    pub name: String,
    pub status: String,
    pub summary: String,
    pub details: Vec<String>,
    pub contract: String,
    pub fixes: Vec<String>,
}

impl GateResult {
    pub fn new(
        name: &str,
        status: &str,
        summary: impl Into<String>,
        details: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.to_string(),
            status: status.to_string(),
            summary: summary.into(),
            details: details.into_iter().map(Into::into).collect(),
            contract: String::new(),
            fixes: Vec::new(),
        }
    }

    pub fn contract(mut self, contract: impl Into<String>) -> Self {
        self.contract = contract.into();
        self
    }

    pub fn fixes(mut self, fixes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.fixes = fixes.into_iter().map(Into::into).collect();
        self
    }

    pub fn passed(&self) -> bool {
        self.status == PASS
    }
}

/// The glyph that stands for a status, so an uncoloured log still reads.
pub fn status_glyph(status: &str) -> &'static str {
    match status {
        PASS => "✓",
        FAIL => "✗",
        INCOMPLETE => "!",
        SKIPPED => "·",
        _ => "?",
    }
}

/// One gate's line: `  [2/6] ✗ size      2 functions over the ceiling`.
pub fn gate_line(
    position: usize,
    total: usize,
    width: usize,
    result: &GateResult,
    style: Style,
) -> String {
    let head = format!(
        "{} {:<width$}",
        status_glyph(&result.status),
        result.name,
        width = width
    );
    format!(
        "  [{position}/{total}] {}  {}",
        style.paint_status(&result.status, &head),
        result.summary
    )
}

/// The line a gate runs under until it answers — rewritten in place.
pub fn gate_progress_line(
    position: usize,
    total: usize,
    width: usize,
    gate: &str,
    style: Style,
) -> String {
    style.dim(&format!(
        "  [{position}/{total}] ⋯ {gate:<width$}  running…",
        width = width
    ))
}

pub fn verdict(results: &[GateResult]) -> String {
    let blocked: Vec<&GateResult> = results.iter().filter(|result| !result.passed()).collect();
    if blocked.is_empty() {
        return SHIP_READY.to_string();
    }
    format!(
        "BLOCKED — {}",
        blocked
            .iter()
            .map(|result| format!("{}={}", result.name, result.status))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn exit_code(results: &[GateResult]) -> i32 {
    let statuses: BTreeSet<&str> = results
        .iter()
        .map(|result| result.status.as_str())
        .collect();
    if statuses.contains(FAIL) {
        return 1;
    }
    if statuses.contains(INCOMPLETE) || statuses.contains(SKIPPED) {
        return 2;
    }
    0
}

#[derive(Debug, Clone, Default)]
pub struct FailureContext<'a> {
    pub target: &'a str,
    pub revision: &'a str,
    pub dirty: &'a str,
    pub base: &'a str,
    pub attempts: Option<i64>,
}

/// The width a terminal gives the text — escape sequences take no columns, so
/// a painted row pads exactly like a plain one.
fn visible_len(text: &str) -> usize {
    let mut width = 0;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            for escape in characters.by_ref() {
                if escape == 'm' {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

/// A labelled box the eye can land on: title on the top rule, one padded row
/// per fact.
pub fn panel(title: &str, rows: &[String], style: Style) -> String {
    let widest = rows.iter().map(|row| visible_len(row)).max().unwrap_or(0);
    let inner = widest.max(title.len() + 1);
    let mut lines = vec![format!(
        "╭─ {} {}╮",
        style.bold(title),
        "─".repeat(inner - title.len() - 1)
    )];
    lines.extend(
        rows.iter()
            .map(|row| format!("│ {row}{} │", " ".repeat(inner - visible_len(row)))),
    );
    lines.push(format!("╰{}╯", "─".repeat(inner + 2)));
    lines.join("\n")
}

/// The verdict, painted by what it says, inside the panel that closes a run.
pub fn render_verdict(final_verdict: &str, style: Style) -> String {
    let painted = if final_verdict == SHIP_READY {
        style.pass(final_verdict)
    } else {
        style.fail(final_verdict)
    };
    panel("verdict", &[painted], style)
}

/// A hash the eye can compare at a glance; the report keeps the full one.
fn short(hash: &str) -> &str {
    match hash.char_indices().nth(8) {
        Some((index, _)) => &hash[..index],
        None => hash,
    }
}

/// The label column of the banner and the failure header.
fn field(label: &str, value: &str) -> String {
    format!("{label:<8} {value}")
}

#[derive(Debug, Clone, Default)]
pub struct BannerContext<'a> {
    pub base: &'a str,
    pub revision: &'a str,
    pub dirty: &'a str,
    pub changed: &'a [String],
    pub selected_how: &'a str,
    pub runner: &'a str,
}

pub fn render_banner(target: &Target, context: &BannerContext<'_>, style: Style) -> String {
    let named = format!(
        "{}{}",
        target.label(),
        if context.selected_how.is_empty() {
            String::new()
        } else {
            format!(" [{}]", context.selected_how)
        }
    );
    let rust = context
        .changed
        .iter()
        .filter(|path| path.ends_with(".rs"))
        .count();
    let mut rows = vec![
        field("target", &named),
        if context.base.is_empty() {
            field("scope", "explicit paths")
        } else {
            field("base", context.base)
        },
        field("revision", short(or_unknown(context.revision))),
        field("dirty", short(or_unknown(context.dirty))),
        field(
            "changed",
            &format!("{} files ({rust} rust)", context.changed.len()),
        ),
    ];
    if !context.runner.is_empty() {
        rows.push(field(
            "runner",
            &format!("{} (declared by .guardrails.toml)", context.runner),
        ));
    }

    let mut lines: Vec<String> = panel("mido", &rows, style)
        .lines()
        .map(str::to_string)
        .collect();
    lines.extend(
        context
            .changed
            .iter()
            .take(MAX_BANNER_FILES)
            .map(|path| style.dim(&format!("  {path}"))),
    );
    if context.changed.len() > MAX_BANNER_FILES {
        lines.push(style.dim(&format!(
            "  … and {} more",
            context.changed.len() - MAX_BANNER_FILES
        )));
    }
    lines.join("\n")
}

fn or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "unknown"
    } else {
        value
    }
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
    let mut rows = vec![
        style.fail(&verdict_of(blocked)),
        field("target", context.target),
        field("revision", short(or_unknown(context.revision))),
        field("dirty", short(or_unknown(context.dirty))),
    ];
    if !context.base.is_empty() {
        rows.push(field("base", context.base));
    }
    rows.push(field(
        "failing",
        &format!("{} of {total} gates", blocked.len()),
    ));

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
        .position(|gate| *gate == result.name)
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
            "      attempts  max {attempts} distinct hypotheses per gate (`.guardrails.toml` [failure])"
        ));
    }
    lines.push(String::new());
    lines
}

/// `BLOCKED — size=FAIL, mutation=INCOMPLETE`, for the results already filtered.
fn verdict_of(blocked: &[&GateResult]) -> String {
    if blocked.is_empty() {
        return SHIP_READY.to_string();
    }
    format!(
        "BLOCKED — {}",
        blocked
            .iter()
            .map(|result| format!("{}={}", result.name, result.status))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub struct ReportContext<'a> {
    pub target: &'a Target,
    pub base: &'a str,
    pub revision: &'a str,
    pub dirty: &'a str,
    pub changed: &'a [String],
    pub runner: &'a str,
}

pub fn render_report_markdown(results: &[GateResult], context: &ReportContext<'_>) -> String {
    let rust = context
        .changed
        .iter()
        .filter(|path| path.ends_with(".rs"))
        .count();
    let mut lines = vec![
        format!("# Guardrails report — {}", context.target.name),
        String::new(),
        format!("VERDICT: {}", verdict(results)),
        format!("revision: `{}`", context.revision),
        format!("dirty state hash: `{}`", context.dirty),
        if context.base.is_empty() {
            "scope: `explicit paths`".to_string()
        } else {
            format!("base: `{}`", context.base)
        },
        format!("target: `{}`", context.target.label()),
    ];
    if !context.runner.is_empty() {
        lines.push(format!("runner: `{}`", context.runner));
    }
    lines.push(format!(
        "changed files: {} ({rust} rust)",
        context.changed.len()
    ));
    lines.push(String::new());
    lines.push("| Gate | Status | Evidence |".to_string());
    lines.push("| --- | --- | --- |".to_string());
    lines.extend(results.iter().map(|result| {
        format!(
            "| {} | {} | {} |",
            result.name, result.status, result.summary
        )
    }));
    lines.push(String::new());

    for result in results {
        lines.push(format!("### {}", result.name));
        lines.push(String::new());
        lines.push(format!("contract: {}", result.contract));
        lines.push(String::new());
        lines.extend(result.details.iter().map(|detail| format!("- {detail}")));
        if !result.fixes.is_empty() {
            lines.push(String::new());
            lines.push("fix:".to_string());
            lines.extend(result.fixes.iter().map(|fix| format!("- {fix}")));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests;
