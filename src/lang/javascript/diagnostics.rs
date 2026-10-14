//! Reading prettier, eslint and tsc output: which files and locations a syntax
//! step reported.

use crate::lang::StepOutcome;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// The prettier summary line starts like a file listing but is not one.
const PRETTIER_SUMMARY: &str = "Code style issues found";

fn prettier_file_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\[warn\] (.+)$").expect("valid pattern"))
}

fn prettier_error_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\[error\] (.+)$").expect("valid pattern"))
}

/// An eslint stylish entry: `  line:col  severity  message  rule`.
fn eslint_entry_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^\s+(\d+):(\d+)\s+(error|warning)\s+(.+?)(?:\s{2,}(\S+))?\s*$")
            .expect("valid pattern")
    })
}

/// `src/foo.ts(12,5): error TS2322: message`, the shape tsc prints off a tty.
fn tsc_plain_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^(.+?)\((\d+),(\d+)\): (error|warning) (TS\d+): (.+)$")
            .expect("valid pattern")
    })
}

/// `src/foo.ts:12:5 - error TS2322: message`, the pretty shape.
fn tsc_pretty_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^(.+?):(\d+):(\d+) - (error|warning) (TS\d+): (.+)$")
            .expect("valid pattern")
    })
}

/// One syntax step's outcome, picked by the step's name (`command` and lint
/// steps report diagnostics; `format` reports what the formatter would rewrite).
pub fn syntax_outcome(
    step: &str,
    command: &str,
    output: &str,
    returncode: i32,
    work_dir: &Path,
) -> StepOutcome {
    if step == "format" {
        format_step(command, output, returncode)
    } else {
        diagnostic_step(step, command, output, returncode, work_dir)
    }
}

fn format_step(command: &str, output: &str, returncode: i32) -> StepOutcome {
    let files = prettier_files(output);
    let mut problems: Vec<String> = files
        .iter()
        .map(|path| format!("{path}: formatting differs (the formatter would rewrite it)"))
        .collect();
    problems.extend(
        prettier_error_pattern()
            .captures_iter(output)
            .map(|captures| format!("prettier: {}", &captures[1])),
    );

    if problems.is_empty() && returncode != 0 {
        return unattributable("format", command, returncode, output);
    }

    StepOutcome {
        details: vec![format!(
            "format: {} file(s) with diffs{}",
            files.len(),
            listing(&files)
        )],
        problems,
    }
}

/// The files a prettier run reported, deduped and in order.
fn prettier_files(output: &str) -> Vec<String> {
    let mut files: Vec<String> = prettier_file_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .filter(|file| !file.starts_with(PRETTIER_SUMMARY))
        .collect();
    files.sort();
    files.dedup();
    files
}

fn diagnostic_step(
    step: &str,
    command: &str,
    output: &str,
    returncode: i32,
    work_dir: &Path,
) -> StepOutcome {
    let mut problems = tsc_diagnostics(output);
    problems.extend(eslint_diagnostics(output, work_dir));

    if problems.is_empty() && returncode != 0 {
        return unattributable(step, command, returncode, output);
    }

    let locations = locations_of(&problems);
    StepOutcome {
        details: vec![format!(
            "{step}: {} diagnostic(s){}",
            problems.len(),
            listing(&locations)
        )],
        problems,
    }
}

/// `file:line:col severity TS-code: message` for every diagnostic tsc printed.
fn tsc_diagnostics(output: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for pattern in [tsc_plain_pattern(), tsc_pretty_pattern()] {
        problems.extend(pattern.captures_iter(output).map(|captures| {
            format!(
                "{}:{}:{} {} {}: {}",
                &captures[1], &captures[2], &captures[3], &captures[4], &captures[5], &captures[6]
            )
        }));
    }
    problems
}

/// `file:line:col severity message (rule)` for every eslint stylish entry,
/// under the file header it was printed under. Absolute paths are made
/// relative to the target — eslint prints resolved paths.
fn eslint_diagnostics(output: &str, work_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let mut current: Option<String> = None;

    for line in output.lines() {
        if let Some(captures) = eslint_entry_pattern().captures(line) {
            if let Some(path) = &current {
                let rule = captures
                    .get(5)
                    .map(|rule| format!(" ({})", rule.as_str()))
                    .unwrap_or_default();
                problems.push(format!(
                    "{path}:{}:{} {} {}{rule}",
                    &captures[1], &captures[2], &captures[3], &captures[4]
                ));
            }
            continue;
        }
        if is_eslint_file_header(line) {
            current = Some(relative_path(line.trim(), work_dir));
        }
    }
    problems
}

/// A file header: flush against the margin, and not the `✖ n problems` summary.
fn is_eslint_file_header(line: &str) -> bool {
    !line.trim().is_empty() && !line.starts_with(char::is_whitespace) && !line.starts_with('✖')
}

/// A path as a tool printed it, relative to the target when it is absolute.
fn relative_path(path: &str, work_dir: &Path) -> String {
    let work_dir = work_dir.to_string_lossy();
    match path.strip_prefix(work_dir.as_ref()) {
        Some(rest) => rest.trim_start_matches('/').to_string(),
        None => path.to_string(),
    }
}

fn location_suffix() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r":\d+:\d+$").expect("valid pattern"))
}

/// The problem locations, deduped, as the evidence line lists them — the file
/// each finding is in.
fn locations_of(problems: &[String]) -> Vec<String> {
    let mut locations: Vec<String> = problems
        .iter()
        .map(|problem| {
            let location = problem.split(' ').next().unwrap_or_default();
            location_suffix().replace(location, "").to_string()
        })
        .collect();
    locations.sort();
    locations.dedup();
    locations
}

fn listing(locations: &[String]) -> String {
    if locations.is_empty() {
        String::new()
    } else {
        format!(" -> {}", locations.join(", "))
    }
}

fn unattributable(step: &str, command: &str, returncode: i32, output: &str) -> StepOutcome {
    let mut details = vec![format!("{step}: the output below could not be attributed")];
    details.extend(crate::process::last_lines_with(
        output,
        10,
        &format!("{step}: "),
    ));
    StepOutcome {
        details,
        problems: vec![format!(
            "`{command}` exited {returncode} with output this runner cannot attribute"
        )],
    }
}

fn severity_prefix() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(error|warning) ").expect("valid pattern"))
}

fn rule_suffix() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r" \([\w@/.-]+\)$").expect("valid pattern"))
}

/// `path:line:col` plus the message, with the severity and the rule folded
/// away: the same finding reported twice is one problem.
fn diagnostic_key(problem: &str) -> String {
    let (location, rest) = problem.split_once(' ').unwrap_or((problem, ""));
    let rest = severity_prefix().replace(rest, "");
    let rest = rule_suffix().replace(&rest, "");
    format!("{location} {rest}")
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

#[cfg(test)]
mod tests;
