//! Reading rustc, clippy and rustfmt output: which files and locations a
//! syntax step reported.

use crate::lang::StepOutcome;
use regex::Regex;
use std::sync::OnceLock;

fn format_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^Diff in (.+?):\d+:").expect("valid pattern"))
}

fn diagnostic_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\s*--> (.+?):(\d+):(\d+)$").expect("valid pattern"))
}

fn severity_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(?:error|warning): ").expect("valid pattern"))
}

/// One syntax step's outcome, picked by the step's name (`command` and lint
/// steps report diagnostics; `format` reports diffs).
pub fn syntax_outcome(step: &str, command: &str, output: &str, returncode: i32) -> StepOutcome {
    if step == "format" {
        format_step(output)
    } else {
        diagnostic_step(step, command, output, returncode)
    }
}

fn format_step(output: &str) -> StepOutcome {
    let files = reported_files(output);

    StepOutcome {
        details: vec![format!(
            "format: {} file(s) with diffs{}",
            files.len(),
            listing(&files)
        )],
        problems: files
            .iter()
            .map(|path| format!("{path}: formatting differs (the formatter would rewrite it)"))
            .collect(),
    }
}

/// The files a formatter run reported, deduped and in order.
fn reported_files(output: &str) -> Vec<String> {
    let mut files: Vec<String> = format_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .collect();
    files.sort();
    files.dedup();
    files
}

fn diagnostic_step(name: &str, command: &str, output: &str, returncode: i32) -> StepOutcome {
    let locations = diagnostic_locations(output);
    let diagnostics = diagnostic_pattern().captures_iter(output).count();

    let mut outcome = StepOutcome {
        details: vec![attribution_line(name, diagnostics, &locations)],
        problems: Vec::new(),
    };

    if diagnostics > 0 {
        outcome.problems = extract_diagnostics(output);
    } else if returncode != 0 {
        // Nothing to attribute means the gate has no evidence, not a free pass.
        outcome.problems = vec![format!(
            "`{command}` exited {returncode} with output this runner cannot attribute"
        )];
        outcome.details.extend(unattributable_lines(name, output));
    }
    outcome
}

/// The diagnostic locations a step reported, deduped.
fn diagnostic_locations(output: &str) -> Vec<String> {
    let mut locations: Vec<String> = diagnostic_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .collect();
    locations.sort();
    locations.dedup();
    locations
}

fn listing(files: &[String]) -> String {
    if files.is_empty() {
        String::new()
    } else {
        format!(" -> {}", files.join(", "))
    }
}

fn attribution_line(name: &str, diagnostics: usize, locations: &[String]) -> String {
    format!("{name}: {diagnostics} diagnostic(s){}", listing(locations))
}

fn unattributable_lines(name: &str, output: &str) -> Vec<String> {
    crate::process::last_lines_with(output, 10, &format!("{name}: "))
}

/// `path:line:col` plus the message, with the severity folded away.
///
/// clippy and `cargo check` report the same lint at the same place, one as
/// `error:` (that is what `-D warnings` does to it) and the other as `warning:`.
/// Counting both would double the problem list without adding a single piece of
/// information.
fn diagnostic_key(problem: &str) -> String {
    let (location, message) = problem.split_once(' ').unwrap_or((problem, ""));
    format!("{location} {}", severity_pattern().replace(message, ""))
}

pub fn dedupe_problems(problems: &[String]) -> Vec<String> {
    let mut unique: Vec<(String, String)> = Vec::new();
    for problem in problems {
        let key = diagnostic_key(problem);
        if let Some(entry) = unique.iter_mut().find(|(known, _)| *known == key) {
            entry.1 = problem.clone();
        } else {
            unique.push((key, problem.clone()));
        }
    }
    unique.into_iter().map(|(_, problem)| problem).collect()
}

/// `file:line:col message` for every diagnostic the step reported.
fn extract_diagnostics(output: &str) -> Vec<String> {
    let lines: Vec<&str> = output.lines().collect();
    let mut problems = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let Some(captures) = diagnostic_pattern().captures(line) else {
            continue;
        };

        let message = diagnostic_message(&lines, index);
        problems.push(
            format!(
                "{}:{}:{} {}",
                &captures[1], &captures[2], &captures[3], message
            )
            .trim_end()
            .to_string(),
        );
    }
    problems
}

/// The `warning:`/`error:` line the location belongs to, up to three lines above it.
fn diagnostic_message(lines: &[&str], index: usize) -> String {
    for back in (index.saturating_sub(3)..index).rev() {
        let candidate = lines[back].trim();
        if candidate.starts_with("warning:") || candidate.starts_with("error:") {
            return candidate.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_differences_are_the_problem() {
        let outcome = syntax_outcome(
            "format",
            "cargo fmt --check",
            "Diff in src/foo.rs:3:\nDiff in src/bar.rs:1:\n",
            1,
        );

        assert_eq!(outcome.problems.len(), 2);
        assert!(outcome.problems[0].contains("src/bar.rs"));
        assert!(outcome.details[0].contains("2 file(s) with diffs -> src/bar.rs, src/foo.rs"));
    }

    #[test]
    fn a_clean_format_pass_has_nothing_to_report() {
        let outcome = syntax_outcome("format", "cargo fmt --check", "", 0);

        assert!(outcome.problems.is_empty());
        assert!(outcome.details[0].contains("0 file(s) with diffs"));
    }

    #[test]
    fn diagnostics_report_the_message_above_the_location() {
        let problems = extract_diagnostics("error: mismatched types\n  --> src/foo.rs:12:5\n");

        assert_eq!(problems, vec!["src/foo.rs:12:5 error: mismatched types"]);
    }

    #[test]
    fn a_diagnostic_without_a_severity_line_keeps_just_its_location() {
        let problems =
            extract_diagnostics("error[E0308]: mismatched types\n  --> src/foo.rs:12:5\n");

        assert_eq!(problems, vec!["src/foo.rs:12:5"]);
    }

    #[test]
    fn every_reported_diagnostic_is_a_problem() {
        let outcome = syntax_outcome(
            "lint",
            "cargo clippy",
            "warning: unused\n  --> src/foo.rs:1:1\nwarning: old\n  --> src/old.rs:2:2\n",
            1,
        );

        assert_eq!(outcome.problems.len(), 2);
        assert!(outcome.details[0].contains("2 diagnostic(s) -> src/foo.rs, src/old.rs"));
    }

    #[test]
    fn a_failing_step_with_no_diagnostics_is_not_a_pass() {
        let outcome = syntax_outcome(
            "lint",
            "cargo clippy",
            "error: could not compile `mido`\n",
            101,
        );

        assert_eq!(outcome.problems.len(), 1);
        assert!(outcome.problems[0].contains("exited 101"));
        assert!(outcome
            .details
            .iter()
            .any(|line| line.starts_with("lint: error:")));
    }

    #[test]
    fn a_passing_step_with_diagnostics_still_reports_them() {
        let outcome = syntax_outcome(
            "typecheck",
            "cargo check",
            "warning: pre-existing\n  --> src/old.rs:2:2\n",
            0,
        );

        assert_eq!(
            outcome.problems,
            vec!["src/old.rs:2:2 warning: pre-existing"]
        );
    }

    #[test]
    fn the_same_diagnostic_from_two_tools_is_kept_once() {
        let problems = dedupe_problems(&[
            "src/foo.rs:9:9 error: unused variable: `x`".to_string(),
            "src/foo.rs:9:9 warning: unused variable: `x`".to_string(),
        ]);

        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn different_diagnostics_are_both_kept() {
        let problems = dedupe_problems(&[
            "src/foo.rs:9:9 error: unused variable: `x`".to_string(),
            "src/foo.rs:4:1 warning: unused import: `std::io`".to_string(),
        ]);

        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0], "src/foo.rs:9:9 error: unused variable: `x`");
    }

    #[test]
    fn the_last_report_of_a_duplicate_wins() {
        let problems = dedupe_problems(&[
            "src/foo.rs:9:9 error: unused variable: `x`".to_string(),
            "src/foo.rs:9:9 warning: unused variable: `x`".to_string(),
        ]);

        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0], "src/foo.rs:9:9 warning: unused variable: `x`");
    }

    #[test]
    fn a_key_folds_the_severity_but_keeps_the_message() {
        assert_eq!(
            diagnostic_key("src/foo.rs:9:9 warning: unused variable: `x`"),
            "src/foo.rs:9:9 unused variable: `x`"
        );
        assert_eq!(diagnostic_key("bare"), "bare ");
    }
}
