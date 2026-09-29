//! Reading a tool's output: which files it touched, which diagnostics land in the change.

use crate::metrics::touches;
use regex::Regex;
use std::sync::OnceLock;

/// What one syntax step made of its output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepOutcome {
    pub details: Vec<String>,
    pub problems: Vec<String>,
    pub crate_wide_debt: bool,
}

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

pub fn format_step(output: &str, changed: &[String]) -> StepOutcome {
    let mut files: Vec<String> = format_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .collect();
    files.sort();
    files.dedup();

    let in_scope: Vec<String> = files
        .iter()
        .filter(|path| touches(path, changed))
        .cloned()
        .collect();

    StepOutcome {
        details: vec![format!(
            "format: {} file(s) with diffs, {} of them changed here{}",
            files.len(),
            in_scope.len(),
            if in_scope.is_empty() {
                String::new()
            } else {
                format!(" -> {}", in_scope.join(", "))
            }
        )],
        problems: in_scope
            .iter()
            .map(|path| format!("{path}: formatting differs (the formatter would rewrite it)"))
            .collect(),
        crate_wide_debt: files.len() > in_scope.len(),
    }
}

pub fn diagnostic_step(
    name: &str,
    command: &str,
    output: &str,
    returncode: i32,
    changed: &[String],
) -> StepOutcome {
    let mut in_scope: Vec<String> = diagnostic_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .filter(|path| touches(path, changed))
        .collect();
    in_scope.sort();
    in_scope.dedup();
    let diagnostics = diagnostic_pattern().captures_iter(output).count();

    let mut outcome = StepOutcome {
        details: vec![format!(
            "{name}: {diagnostics} diagnostic(s), {} of them in changed files{}",
            in_scope.len(),
            if in_scope.is_empty() {
                String::new()
            } else {
                format!(" -> {}", in_scope.join(", "))
            }
        )],
        crate_wide_debt: diagnostics > in_scope.len(),
        problems: Vec::new(),
    };

    if !in_scope.is_empty() {
        outcome.problems = extract_diagnostics(output, changed);
    } else if returncode != 0 && diagnostics == 0 {
        // Nothing to attribute means the gate has no evidence, not a free pass.
        outcome.problems = vec![format!(
            "`{command}` exited {returncode} with output this runner cannot attribute"
        )];
        outcome.details.extend(unattributable_lines(name, output));
    }
    outcome
}

fn unattributable_lines(name: &str, output: &str) -> Vec<String> {
    let lines: Vec<&str> = output.trim().lines().collect();
    let start = lines.len().saturating_sub(10);
    lines[start..]
        .iter()
        .map(|line| format!("{name}: {}", line.trim()))
        .collect()
}

/// `path:line:col` plus the message, with the severity folded away.
///
/// clippy and `cargo check` report the same lint at the same place, one as
/// `error:` (that is what `-D warnings` does to it) and the other as `warning:`.
/// Counting both would double the problem list without adding a single piece of
/// information.
pub fn diagnostic_key(problem: &str) -> String {
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

/// `file:line:col message` for diagnostics landing in the changed files.
pub fn extract_diagnostics(output: &str, changed: &[String]) -> Vec<String> {
    let lines: Vec<&str> = output.lines().collect();
    let mut problems = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let Some(captures) = diagnostic_pattern().captures(line) else {
            continue;
        };
        if !touches(&captures[1], changed) {
            continue;
        }

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
