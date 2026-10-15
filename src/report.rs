//! Verdicts and the reports the next step reads.

pub mod markdown;
pub mod panel;
pub mod views;

pub use markdown::{render_report_markdown, ReportContext};
pub use panel::{gate_line, gate_progress_line, panel, render_verdict, status_glyph, visible_len};
pub use views::{render_banner, render_failure, BannerContext, FailureContext};

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

    /// Whether this result stands in the run's way.
    ///
    /// A gate the config waived reports `SKIPPED`: it did not pass, and it is
    /// not a blocker either — the waiver is in writing, in the file its own
    /// contract names, and the verdict names the gate.
    pub fn blocks(&self) -> bool {
        self.status != PASS && self.status != SKIPPED
    }
}

pub fn verdict(results: &[GateResult]) -> String {
    let blocked: Vec<&GateResult> = results.iter().filter(|result| result.blocks()).collect();
    blocked_verdict(&blocked)
}

/// The gates the config waived, in ladder order. The verdict panel, the json
/// and the report name them, so a green run with a waiver is never read as a
/// green run without one.
pub fn waived(results: &[GateResult]) -> Vec<&str> {
    results
        .iter()
        .filter(|result| result.status == SKIPPED)
        .map(|result| result.name.as_str())
        .collect()
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

/// 1 when a gate failed, 2 when a gate could not run to a verdict. A gate the
/// config waived is neither — the ladder without it is the ladder that ran.
pub fn exit_code(results: &[GateResult]) -> i32 {
    if results.iter().any(|result| result.status == FAIL) {
        return 1;
    }
    if results.iter().any(|result| result.status == INCOMPLETE) {
        return 2;
    }
    0
}

#[cfg(test)]
mod tests;
