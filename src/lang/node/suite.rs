//! node:test, vitest and jest output, and the commands the tests gate runs for
//! a target.

use crate::config::Config;
use crate::lang::TestSummary;
use crate::targets::Target;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// The lockfiles that name a package manager, in the order they are trusted.
const LOCKFILES: [(&str, &str); 5] = [
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "yarn"),
    ("bun.lockb", "bun"),
    ("bun.lock", "bun"),
    ("package-lock.json", "npm"),
];

/// The package manager the repo declared: the first lockfile next to the
/// target, then at the repo root; npm when there is none.
pub fn package_manager(target_dir: &Path, repo: &Path) -> &'static str {
    for directory in [target_dir, repo] {
        for (lockfile, manager) in LOCKFILES {
            if directory.join(lockfile).exists() {
                return manager;
            }
        }
    }
    "npm"
}

/// The commands the tests gate runs for this target.
///
/// An explicit `[tests] command` wins. Otherwise the suite is the repo's own:
/// `<package manager> test`, so a pnpm workspace tests through pnpm.
pub fn test_commands(config: &Config, target: &Target, repo: &Path) -> Vec<Vec<String>> {
    if let Some(configured) = config.argv("tests", "command", target) {
        return vec![configured];
    }

    let manager = package_manager(&target.dir(repo), repo);
    vec![vec![manager.to_string(), "test".to_string()]]
}

/// The summary the output carries, in the order the runners are recognized:
/// the node:test spec and tap reporters, vitest, jest. `None` when the output
/// printed none of them.
pub fn test_summary(output: &str) -> Option<TestSummary> {
    node_test_summary(output)
        .or_else(|| vitest_summary(output))
        .or_else(|| jest_summary(output))
}

fn node_pass_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^(?:ℹ|#) pass (\d+)$").expect("valid pattern"))
}

fn node_fail_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^(?:ℹ|#) fail (\d+)$").expect("valid pattern"))
}

/// The failure lines of the spec reporter's `✖ failing tests:` section.
fn node_failure_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^✖ (.+?)(?: \([^)]*\))?$").expect("valid pattern"))
}

/// The tap reporter's `not ok N - name` lines.
fn node_tap_failure_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^not ok \d+ - (.+)$").expect("valid pattern"))
}

fn node_test_summary(output: &str) -> Option<TestSummary> {
    let passed: i64 = node_pass_pattern()
        .captures(output)?
        .get(1)?
        .as_str()
        .parse()
        .ok()?;
    let failed: i64 = node_fail_pattern()
        .captures(output)?
        .get(1)?
        .as_str()
        .parse()
        .ok()?;

    let mut failed_names: Vec<String> = Vec::new();
    if let Some(section) = output.split("✖ failing tests:").nth(1) {
        failed_names.extend(
            node_failure_pattern()
                .captures_iter(section)
                .map(|captures| captures[1].to_string()),
        );
    }
    failed_names.extend(
        node_tap_failure_pattern()
            .captures_iter(output)
            .map(|captures| captures[1].to_string()),
    );
    failed_names.sort();
    failed_names.dedup();

    Some(TestSummary {
        passed,
        failed,
        failed_names,
    })
}

fn vitest_counts_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^\s*Tests\s+(?:(\d+) failed \| (\d+) passed|(\d+) failed|(\d+) passed)\b")
            .expect("valid pattern")
    })
}

fn vitest_failure_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\s*FAIL\s+(.+)$").expect("valid pattern"))
}

fn vitest_summary(output: &str) -> Option<TestSummary> {
    let captures = vitest_counts_pattern().captures(output)?;
    let number = |index: usize| -> i64 {
        captures
            .get(index)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or_default()
    };

    let mut failed_names: Vec<String> = vitest_failure_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].trim().to_string())
        .collect();
    failed_names.sort();
    failed_names.dedup();

    Some(TestSummary {
        passed: number(2).max(number(4)),
        failed: number(1).max(number(3)),
        failed_names,
    })
}

fn jest_counts_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\s*Tests:\s+(.+)$").expect("valid pattern"))
}

fn jest_number_pattern(what: &str) -> &'static Regex {
    static FAILED: OnceLock<Regex> = OnceLock::new();
    static PASSED: OnceLock<Regex> = OnceLock::new();
    if what == "failed" {
        FAILED.get_or_init(|| Regex::new(r"(\d+) failed").expect("valid pattern"))
    } else {
        PASSED.get_or_init(|| Regex::new(r"(\d+) passed").expect("valid pattern"))
    }
}

fn jest_failure_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?m)^\s*● (.+)$").expect("valid pattern"))
}

fn jest_summary(output: &str) -> Option<TestSummary> {
    let counts = jest_counts_pattern().captures(output)?.get(1)?.as_str();
    let number = |what: &str| -> i64 {
        jest_number_pattern(what)
            .captures(counts)
            .and_then(|captures| captures[1].parse().ok())
            .unwrap_or_default()
    };

    let mut failed_names: Vec<String> = jest_failure_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].trim().to_string())
        .filter(|name| name != "Test suite failed to run")
        .collect();
    failed_names.sort();
    failed_names.dedup();

    Some(TestSummary {
        passed: number("passed"),
        failed: number("failed"),
        failed_names,
    })
}
