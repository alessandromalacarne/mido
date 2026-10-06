use super::*;
use crate::report::{FAIL, INCOMPLETE, PASS, SKIPPED};
use std::ffi::OsString;
use std::sync::Mutex;

/// The environment is process-wide, so the tests that touch it take turns.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Sets `NO_COLOR`/`TERM` for one test and puts them back on drop.
struct Env {
    no_color: Option<OsString>,
    term: Option<OsString>,
}

impl Env {
    fn set(no_color: Option<&str>, term: Option<&str>) -> Self {
        let previous = Self {
            no_color: std::env::var_os("NO_COLOR"),
            term: std::env::var_os("TERM"),
        };
        set_var("NO_COLOR", no_color);
        set_var("TERM", term);
        previous
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        restore("NO_COLOR", self.no_color.take());
        restore("TERM", self.term.take());
    }
}

fn set_var(name: &str, value: Option<&str>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

fn restore(name: &str, value: Option<OsString>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

#[test]
fn a_plain_style_writes_no_escape_codes() {
    let style = Style::plain();

    assert_eq!(style.pass(PASS), PASS);
    assert_eq!(style.fail("size=FAIL"), "size=FAIL");
    assert_eq!(style.dim("src/foo.rs"), "src/foo.rs");
    assert_eq!(style.paint_status(INCOMPLETE, INCOMPLETE), INCOMPLETE);
}

#[test]
fn a_coloured_style_paints_each_status() {
    let style = Style::colored();

    assert_eq!(style.pass(PASS), "\u{1b}[32mPASS\u{1b}[0m");
    assert_eq!(style.fail(FAIL), "\u{1b}[31mFAIL\u{1b}[0m");
    assert_eq!(
        style.paint_status(INCOMPLETE, INCOMPLETE),
        "\u{1b}[33mINCOMPLETE\u{1b}[0m"
    );
    assert_eq!(
        style.paint_status(SKIPPED, SKIPPED),
        "\u{1b}[2mSKIPPED\u{1b}[0m"
    );
    assert_eq!(style.bold("mido"), "\u{1b}[1mmido\u{1b}[0m");
    assert_eq!(
        style.paint_status(FAIL, "✗ size"),
        "\u{1b}[31m✗ size\u{1b}[0m"
    );
    assert_eq!(
        style.paint_status(PASS, "✓ tests"),
        "\u{1b}[32m✓ tests\u{1b}[0m"
    );
}

#[test]
fn a_status_outside_the_vocabulary_is_left_alone() {
    assert_eq!(
        Style::colored().paint_status("WHATEVER", "WHATEVER"),
        "WHATEVER"
    );
}

#[test]
fn colour_is_off_when_nobody_is_looking_at_a_terminal() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = Env::set(None, Some("xterm-256color"));

    assert!(!Style::detect_with(false).on());
}

#[test]
fn a_dumb_terminal_or_a_no_color_variable_turns_colour_off() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let _plain_term = Env::set(None, Some("xterm-256color"));
    assert!(Style::detect_with(true).on());

    let _no_color = Env::set(Some("1"), Some("xterm-256color"));
    assert!(!Style::detect_with(true).on());

    let _dumb = Env::set(None, Some("dumb"));
    assert!(!Style::detect_with(true).on());

    let _empty = Env::set(Some(""), Some("xterm-256color"));
    assert!(
        Style::detect_with(true).on(),
        "an empty NO_COLOR is not an opt-out"
    );
}
