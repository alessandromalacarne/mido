//! Verdicts and the reports the next step reads.

use crate::targets::Target;
use std::collections::BTreeSet;

pub const PASS: &str = "PASS";
pub const FAIL: &str = "FAIL";
pub const INCOMPLETE: &str = "INCOMPLETE";
pub const SKIPPED: &str = "SKIPPED";

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

pub fn verdict(results: &[GateResult]) -> String {
    let blocked: Vec<&GateResult> = results.iter().filter(|result| !result.passed()).collect();
    if blocked.is_empty() {
        return "SHIP-READY".to_string();
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

pub fn render_banner(
    target: &Target,
    base: &str,
    revision: &str,
    dirty: &str,
    changed: &[String],
    selected_how: &str,
    runner: &str,
) -> String {
    let mut lines = vec![format!(
        "guardrails — target {}{}",
        target.label(),
        if selected_how.is_empty() {
            String::new()
        } else {
            format!(" [{selected_how}]")
        }
    )];
    if !runner.is_empty() {
        lines.push(format!("runner {runner} (declared by .guardrails.toml)"));
    }

    let rust = changed.iter().filter(|path| path.ends_with(".rs")).count();
    lines.push(format!(
        "base {base} | changed files: {} ({rust} rust)",
        changed.len()
    ));
    lines.push(format!(
        "revision {} | dirty state hash {}",
        or_unknown(revision),
        or_unknown(dirty)
    ));
    lines.extend(
        changed
            .iter()
            .take(MAX_BANNER_FILES)
            .map(|path| format!("  {path}")),
    );
    if changed.len() > MAX_BANNER_FILES {
        lines.push(format!("  … and {} more", changed.len() - MAX_BANNER_FILES));
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

pub fn render_failure(results: &[GateResult], context: &FailureContext<'_>) -> String {
    let blocked: Vec<&GateResult> = results.iter().filter(|result| !result.passed()).collect();
    let mut lines = failure_header(&blocked, results.len(), context);

    for result in &blocked {
        lines.extend(render_gate_block(result, context));
    }

    if blocked.iter().any(|result| result.status == INCOMPLETE) {
        lines.push("note: INCOMPLETE means the gate did not run to a verdict — a missing tool, an unanalyzable file or a".to_string());
        lines.push("      timeout is never a pass. Fix the tooling (nix develop) and re-run before handing this off.".to_string());
        lines.push(String::new());
    }

    format!("{}\n", lines.join("\n").trim_end())
}

fn failure_header(
    blocked: &[&GateResult],
    total: usize,
    context: &FailureContext<'_>,
) -> Vec<String> {
    vec![
        String::new(),
        format!("FAILURE REPORT — {}", verdict_of(blocked)),
        format!("  target   {}", context.target),
        format!(
            "  revision {} | dirty state hash {}{}",
            or_unknown(context.revision),
            or_unknown(context.dirty),
            if context.base.is_empty() {
                String::new()
            } else {
                format!(" | base {}", context.base)
            }
        ),
        format!(
            "  failing  {} of {total} gate(s): {}",
            blocked.len(),
            blocked
                .iter()
                .map(|result| format!("{}={}", result.name, result.status))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        String::new(),
    ]
}

/// One blocked gate: position in the ladder, contract, evidence, fix.
fn render_gate_block(result: &GateResult, context: &FailureContext<'_>) -> Vec<String> {
    let order = GATES
        .iter()
        .position(|gate| *gate == result.name)
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut lines = vec![
        format!(
            "gate {order}/{} — {} — {}",
            GATES.len(),
            result.name,
            result.status
        ),
        format!("  summary   {}", result.summary),
    ];

    if !result.contract.is_empty() {
        lines.push(format!("  contract  {}", result.contract));
    }
    if !result.details.is_empty() {
        lines.push("  evidence".to_string());
        lines.extend(
            result
                .details
                .iter()
                .map(|detail| format!("    - {detail}")),
        );
    }
    if !result.fixes.is_empty() {
        lines.push("  fix".to_string());
        lines.extend(result.fixes.iter().map(|fix| format!("    - {fix}")));
    }
    if let Some(attempts) = context.attempts {
        lines.push(format!(
            "  attempts  max {attempts} distinct hypotheses per gate (`.guardrails.toml` [failure])"
        ));
    }
    lines.push(String::new());
    lines
}

/// `BLOCKED — size=FAIL, mutation=INCOMPLETE`, for the results already filtered.
fn verdict_of(blocked: &[&GateResult]) -> String {
    if blocked.is_empty() {
        return "SHIP-READY".to_string();
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
        format!("base: `{}`", context.base),
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
