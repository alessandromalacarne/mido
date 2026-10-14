//! The javascript module: the embedded javascript baseline, and what makes a
//! repo javascript. The npm-ecosystem tooling it drives — package.json
//! targets, prettier/eslint/tsc parsing, the package manager's test script and
//! its runners, stryker, coverage — lives in the shared `node` module, which
//! the typescript module drives too.
//!
//! The module covers plain javascript (`.js`, `.mjs`, `.cjs`, `.jsx`).
//! Typescript is not one of its source extensions: a `.ts` repo wants a
//! typecheck default this module does not have, and a module that stays silent
//! about types would be lying about the syntax gate — the `typescript` module
//! owns that case.

pub const NAME: &str = "javascript";
pub const MANIFEST: &str = crate::lang::node::MANIFEST;
pub const SOURCE_LABEL: &str = "javascript";
/// The extensions the module treats as source.
pub const SOURCE_EXTS: [&str; 4] = [".js", ".mjs", ".cjs", ".jsx"];
/// The binary whose presence means the dev environment is set up.
pub const ENV_TOOL: &str = "node";

/// Whether a path is a javascript source file.
pub fn is_source(path: &str) -> bool {
    SOURCE_EXTS.iter().any(|ext| path.ends_with(ext))
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

#[cfg(test)]
mod tests;
