use crate::style::Style;
use std::fmt;

/// A run that cannot produce a verdict. Rendered as `error: ...` and exit 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailsError {
    message: String,
    details: Vec<String>,
    hint: Option<String>,
}

impl GuardrailsError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: Vec::new(),
            hint: None,
        }
    }

    /// `.mido.toml` is unusable — a typo must not silently disable a gate.
    pub fn config(message: impl Into<String>) -> Self {
        Self::new(message)
    }

    /// The requested run does not make sense (unknown package, no manifest).
    pub fn setup(message: impl Into<String>) -> Self {
        Self::new(message)
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details.push(detail.into());
        self
    }

    pub fn details(mut self, details: impl IntoIterator<Item = String>) -> Self {
        self.details.extend(details);
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn exit_code(&self) -> i32 {
        2
    }

    pub fn render(&self) -> String {
        self.render_styled(Style::plain())
    }

    /// `error:` leads in red; the hint steps back, when a terminal is watching.
    pub fn render_styled(&self, style: Style) -> String {
        let mut lines = vec![format!("{} {}", style.fail("error:"), self.message)];
        lines.extend(
            self.details
                .iter()
                .map(|detail| style.dim(&format!("  {detail}"))),
        );
        if let Some(hint) = &self.hint {
            lines.push(style.dim(&format!("  hint: {hint}")));
        }
        lines.join("\n")
    }
}

impl fmt::Display for GuardrailsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render())
    }
}

impl std::error::Error for GuardrailsError {}

/// A gate failed. Carries the detailed report for the caller to print, and the
/// exit code its verdict implies: `1` when a gate `FAIL`ed, `2` when none could
/// run to a verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateFailure {
    pub report: String,
    pub verdict: String,
    code: i32,
}

impl GateFailure {
    pub fn new(report: impl Into<String>, verdict: impl Into<String>, code: i32) -> Self {
        Self {
            report: report.into(),
            verdict: verdict.into(),
            code,
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.code
    }
}

impl fmt::Display for GateFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.report)
    }
}

impl std::error::Error for GateFailure {}

/// Either a run that could not produce a verdict (exit 2) or a blocked one,
/// which exits by what blocked it: 1 for a `FAIL`, 2 for a gate that never
/// reached a verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    Error(GuardrailsError),
    Failure(GateFailure),
}

impl RunError {
    pub fn exit_code(&self) -> i32 {
        match self {
            RunError::Error(error) => error.exit_code(),
            RunError::Failure(failure) => failure.exit_code(),
        }
    }
}

impl From<GuardrailsError> for RunError {
    fn from(error: GuardrailsError) -> Self {
        RunError::Error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_error_renders_message_details_and_hint() {
        let error = GuardrailsError::setup("cannot measure anything here")
            .detail("no Cargo.toml under `scripts`")
            .hint("pass -p frontend");

        let rendered = error.render();

        assert!(rendered.contains("error: cannot measure anything here"));
        assert!(rendered.contains("no Cargo.toml under `scripts`"));
        assert!(rendered.contains("hint: pass -p frontend"));
    }

    #[test]
    fn config_error_renders_the_offending_line() {
        let error = GuardrailsError::config("`.mido.toml` is not valid")
            .detail("line 5: unknown key `min_mi` in [analysis]");

        assert!(error.render().contains("line 5"));
    }

    #[test]
    fn errors_exit_two_and_blocked_runs_carry_their_code() {
        assert_eq!(GuardrailsError::setup("boom").exit_code(), 2);
        assert_eq!(GuardrailsError::config("boom").exit_code(), 2);
        assert_eq!(GateFailure::new("report", "BLOCKED", 1).exit_code(), 1);
        assert_eq!(GateFailure::new("report", "BLOCKED", 2).exit_code(), 2);
    }

    #[test]
    fn display_matches_render() {
        let error = GuardrailsError::setup("boom").detail("why");

        assert_eq!(error.to_string(), error.render());
    }

    #[test]
    fn a_styled_error_paints_the_lead_in_and_steps_the_hint_back() {
        let error = GuardrailsError::setup("boom").hint("pass -p frontend");

        let rendered = error.render_styled(Style::colored());

        assert!(
            rendered.starts_with("\u{1b}[31merror:\u{1b}[0m boom"),
            "{rendered:?}"
        );
        assert!(
            rendered.ends_with("\u{1b}[2m  hint: pass -p frontend\u{1b}[0m"),
            "{rendered:?}"
        );
    }
}
