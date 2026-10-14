//! The node ecosystem, shared by the javascript and typescript modules:
//! package.json targets, prettier/eslint/tsc output, the package manager's
//! test script and its runners, stryker. Everything here behaves the same for
//! a `.js` and a `.ts` repo; the modules own what differs between them.

pub mod diagnostics;
pub mod mutation;
pub mod suite;
pub mod targets;

use crate::error::GuardrailsError;
use std::path::Path;

/// The manifest every npm package carries; targets are scoped by it.
pub const MANIFEST: &str = "package.json";

/// The error a run gets when there is no package to measure.
pub fn no_targets_error(repo: &Path) -> GuardrailsError {
    GuardrailsError::setup(format!("no npm package found under {}", repo.display()))
        .hint("the ladder measures package.json targets; run it from the repo root")
}

/// The bare command a gate runs when no config applies: a standalone package
/// (one outside the workspace patterns) never inherits root commands.
pub fn fallback_argv(gate: &str, key: &str) -> Option<Vec<String>> {
    let argv = match (gate, key) {
        ("syntax", "format") => &["npx", "--no-install", "prettier", "--check", "."][..],
        ("syntax", "lint") => &["npx", "--no-install", "eslint", "."][..],
        ("tests", "command") => &["npm", "test"][..],
        ("coverage", "command") => &[
            "npx",
            "--no-install",
            "c8",
            "--reporter=lcovonly",
            "--reports-dir",
            ".",
            "npm",
            "test",
        ][..],
        ("mutation", "command") => &["npx", "--no-install", "stryker", "run"][..],
        _ => return None,
    };
    Some(argv.iter().map(|arg| (*arg).to_string()).collect())
}
