//! ANSI styling — colour only while a human is looking at the terminal.

use crate::report::{FAIL, INCOMPLETE, PASS, SKIPPED};
use std::io::IsTerminal;

const RESET: &str = "\u{1b}[0m";

/// Whether to paint the output. Everything else is the same either way, so a
/// piped run stays byte-for-byte plain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    color: bool,
}

impl Style {
    pub const fn plain() -> Self {
        Self { color: false }
    }

    pub const fn colored() -> Self {
        Self { color: true }
    }

    /// Colour when stdout is a terminal that did not opt out.
    pub fn detect() -> Self {
        Self::detect_with(std::io::stdout().is_terminal())
    }

    pub fn detect_with(tty: bool) -> Self {
        Self {
            color: tty && color_env_allows(),
        }
    }

    /// Whether this style paints at all — also the signal that a live progress
    /// line can be rewritten in place.
    pub fn on(self) -> bool {
        self.color
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\u{1b}[{code}m{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    pub fn pass(self, text: &str) -> String {
        self.paint("32", text)
    }

    pub fn fail(self, text: &str) -> String {
        self.paint("31", text)
    }

    pub fn warn(self, text: &str) -> String {
        self.paint("33", text)
    }

    pub fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }

    pub fn bold(self, text: &str) -> String {
        self.paint("1", text)
    }

    /// The gate vocabulary decides the colour, not the caller.
    pub fn status(self, status: &str) -> String {
        self.paint_status(status, status)
    }

    /// Paints arbitrary text — a glyph, a gate name — in a status' colour.
    pub fn paint_status(self, status: &str, text: &str) -> String {
        match status {
            PASS => self.pass(text),
            FAIL => self.fail(text),
            INCOMPLETE => self.warn(text),
            SKIPPED => self.dim(text),
            _ => text.to_string(),
        }
    }
}

/// `NO_COLOR` (non-empty) and `TERM=dumb` are opt-outs.
fn color_env_allows() -> bool {
    let opted_out = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    !opted_out && std::env::var("TERM").as_deref() != Ok("dumb")
}

#[cfg(test)]
mod tests;
