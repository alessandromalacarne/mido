//! coverage: the command the node modules derive for a repo — the test runner
//! its devDependencies name — and the lcov report the gate reads from it.

use crate::config::Config;
use crate::targets::Target;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

/// The report directory every derived command names, relative to the target:
/// vitest refuses the project root itself (it deletes the directory before the
/// run), and `coverage/` is the ecosystem's own artifact directory.
const REPORTS_DIR: &str = "coverage";
const REPORT_FILE: &str = "lcov.info";

/// vitest's own coverage, lcov written into the reports directory.
const VITEST: [&str; 7] = [
    "npx",
    "--no-install",
    "vitest",
    "run",
    "--coverage",
    "--coverage.reporter=lcov",
    "--coverage.reportsDirectory=coverage",
];

/// jest's coverage, lcov written into the reports directory.
const JEST: [&str; 6] = [
    "npx",
    "--no-install",
    "jest",
    "--coverage",
    "--coverageReporters=lcov",
    "--coverageDirectory=coverage",
];

/// The argv the coverage gate runs: the config's own first, then what the
/// repo's devDependencies name — vitest's or jest's own coverage, or c8
/// wrapping the package manager's test script. Every variant writes
/// `coverage/lcov.info`, where the gate reads it.
pub fn command(config: &Config, target: &Target, repo: &Path) -> Option<Vec<String>> {
    if let Some(configured) = config.argv("coverage", "command", target) {
        return Some(configured);
    }

    Some(detected(&target.dir(repo), repo))
}

/// Where the command says its report lands: the reports directory its flags
/// name plus `lcov.info`, the node runners' own `coverage/` when it names none.
pub fn report_path(argv: &[String]) -> Option<PathBuf> {
    let directory = flag_value(
        argv,
        &[
            "--coverage.reportsDirectory",
            "--coverageDirectory",
            "--reports-dir",
            "--report-dir",
        ],
    )
    .unwrap_or_else(|| PathBuf::from(REPORTS_DIR));

    Some(directory.join(REPORT_FILE))
}

/// The value of the first flag named, in either the `--flag value` or
/// `--flag=value` form.
fn flag_value(argv: &[String], flags: &[&str]) -> Option<PathBuf> {
    for (index, arg) in argv.iter().enumerate() {
        for flag in flags {
            if arg == flag {
                return argv.get(index + 1).map(PathBuf::from);
            }
            if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Vitest,
    Jest,
    C8,
}

fn detected(target_dir: &Path, repo: &Path) -> Vec<String> {
    match runner(target_dir, repo) {
        Runner::Vitest => argv(&VITEST),
        Runner::Jest => argv(&JEST),
        Runner::C8 => c8(target_dir, repo),
    }
}

/// The runner the devDependencies name: the target's own package.json first,
/// then the root's — a hoisted monorepo keeps its tooling there; c8 when no
/// package names one.
fn runner(target_dir: &Path, repo: &Path) -> Runner {
    for directory in [target_dir, repo] {
        if let Some(runner) = runner_in(&directory.join(super::MANIFEST)) {
            return runner;
        }
    }
    Runner::C8
}

fn runner_in(manifest: &Path) -> Option<Runner> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let document: Json = serde_json::from_str(&text).ok()?;
    let dependencies = document.get("devDependencies")?.as_object()?;

    if dependencies.contains_key("vitest") {
        return Some(Runner::Vitest);
    }
    if dependencies.contains_key("jest") {
        return Some(Runner::Jest);
    }
    None
}

/// c8 wraps the package manager's test script: the runner-agnostic fallback,
/// writing the same lcov into the reports directory.
fn c8(target_dir: &Path, repo: &Path) -> Vec<String> {
    let manager = super::suite::package_manager(target_dir, repo);
    let mut argv = argv(&[
        "npx",
        "--no-install",
        "c8",
        "--reporter=lcovonly",
        "--reports-dir",
        REPORTS_DIR,
    ]);
    argv.push(manager.to_string());
    argv.push("test".to_string());
    argv
}

fn argv(command: &[&str]) -> Vec<String> {
    command.iter().map(|arg| (*arg).to_string()).collect()
}

#[cfg(test)]
mod tests;
