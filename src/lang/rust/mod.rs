//! The rust module: cargo targets, rustc/clippy diagnostics, libtest output,
//! cargo-mutants, rust-code-analysis — and the embedded rust baseline config.

pub mod aid;
pub mod analysis;
pub mod diagnostics;
pub mod mutation;
pub mod suite;
pub mod targets;

pub const NAME: &str = "rust";
pub const MANIFEST: &str = "Cargo.toml";
pub const SOURCE_LABEL: &str = "rust";
pub const SOURCE_EXT: &str = ".rs";
pub const ENV_TOOL: &str = "cargo";

/// Whether a path is a rust source file.
pub fn is_source(path: &str) -> bool {
    path.ends_with(SOURCE_EXT)
}

/// The error a run gets when there is nothing cargo-shaped to measure.
pub fn no_targets_error(repo: &std::path::Path) -> crate::error::GuardrailsError {
    crate::error::GuardrailsError::setup(format!("no cargo target found under {}", repo.display()))
        .hint("the ladder measures cargo targets; run it from the repo root")
}

/// The embedded baseline, as text — the same document as the repo's `.mido.toml`.
pub const DEFAULTS: &str = include_str!("defaults.toml");

/// The rust baseline the ladder runs when `.mido.toml` is silent (or absent).
pub fn default_config() -> &'static toml::Table {
    static DEFAULTS_TABLE: std::sync::OnceLock<toml::Table> = std::sync::OnceLock::new();
    DEFAULTS_TABLE.get_or_init(|| {
        DEFAULTS
            .parse()
            .expect("the embedded defaults are valid toml")
    })
}

/// The bare cargo command a gate runs when no config applies: a standalone
/// crate (excluded from the root manifest) never inherits root commands.
pub fn fallback_argv(gate: &str, key: &str) -> Option<Vec<String>> {
    let argv = match (gate, key) {
        ("syntax", "format") => &["cargo", "fmt", "--check"][..],
        ("syntax", "lint") => &["cargo", "clippy", "--all-targets", "--", "-D", "warnings"][..],
        ("syntax", "typecheck") => &["cargo", "check"][..],
        ("tests", "command") => &["cargo", "test"][..],
        ("coverage", "command") => &["cargo", "llvm-cov", "--lcov", "--output-path", "{lcov}"][..],
        ("mutation", "command") => &["cargo", "mutants"][..],
        _ => return None,
    };
    Some(argv.iter().map(|arg| (*arg).to_string()).collect())
}

#[cfg(test)]
mod tests;
