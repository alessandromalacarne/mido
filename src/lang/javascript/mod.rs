//! The javascript module: package.json targets, prettier/eslint/tsc diagnostics,
//! node:test/vitest/jest output, stryker — and the embedded javascript baseline.
//!
//! The module covers plain javascript (`.js`, `.mjs`, `.cjs`, `.jsx`).
//! Typescript is not one of its source extensions: a `.ts` repo wants a
//! typecheck default this module does not have, and a module that stays silent
//! about types would be lying about the syntax gate.

pub mod diagnostics;
pub mod mutation;
pub mod suite;
pub mod targets;

use crate::error::GuardrailsError;
use crate::process::Runner;
use crate::targets::Target;
use std::io::Write;
use std::path::Path;

pub const NAME: &str = "javascript";
pub const MANIFEST: &str = "package.json";
pub const SOURCE_LABEL: &str = "javascript";
/// The extensions the module treats as source.
pub const SOURCE_EXTS: [&str; 4] = [".js", ".mjs", ".cjs", ".jsx"];
/// The binary whose presence means the dev environment is set up.
pub const ENV_TOOL: &str = "node";

/// Whether a path is a javascript source file.
pub fn is_source(path: &str) -> bool {
    SOURCE_EXTS.iter().any(|ext| path.ends_with(ext))
}

/// The error a run gets when there is no package to measure.
pub fn no_targets_error(repo: &Path) -> GuardrailsError {
    GuardrailsError::setup(format!("no npm package found under {}", repo.display()))
        .hint("the ladder measures package.json targets; run it from the repo root")
}

/// The embedded baseline, as text — the same document as a repo's `.mido.toml`.
pub const DEFAULTS: &str = include_str!("defaults.toml");

/// The javascript baseline the ladder runs when `.mido.toml` is silent (or absent).
pub fn default_config() -> &'static toml::Table {
    static DEFAULTS_TABLE: std::sync::OnceLock<toml::Table> = std::sync::OnceLock::new();
    DEFAULTS_TABLE.get_or_init(|| {
        DEFAULTS
            .parse()
            .expect("the embedded defaults are valid toml")
    })
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

/// Check the package's manifest is there; javascript needs no workspace aid.
pub fn validate_target_setup(
    _runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    _apply_aid: bool,
    _out: &mut dyn Write,
) -> Result<(), GuardrailsError> {
    crate::lang::ensure_manifest(repo, target)
}

#[cfg(test)]
mod tests;
