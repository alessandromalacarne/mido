//! Verdicts and the reports the next step reads.

pub mod markdown;
pub mod panel;
pub mod views;

pub use markdown::{render_report_markdown, ReportContext};
pub use panel::{gate_line, gate_progress_line, panel, render_verdict, status_glyph, visible_len};
pub use views::{render_banner, render_failure, BannerContext, FailureContext};

use std::collections::BTreeSet;

pub const PASS: &str = "PASS";
pub const FAIL: &str = "FAIL";
pub const INCOMPLETE: &str = "INCOMPLETE";
pub const SKIPPED: &str = "SKIPPED";
pub const SHIP_READY: &str = "SHIP-READY";

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
    blocked_verdict(&blocked)
}

/// `BLOCKED — size=FAIL, mutation=INCOMPLETE`, for results already filtered.
pub(super) fn blocked_verdict(blocked: &[&GateResult]) -> String {
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

#[cfg(test)]
mod tests;
