//! The typescript module: what makes a repo typescript — a root
//! tsconfig.json, the `.ts`/`.tsx` source set, the tsc typecheck default. The
//! npm-ecosystem tooling it drives (package.json targets, prettier/eslint/tsc
//! parsing, the package manager's test runners, stryker, coverage) lives in
//! the shared `node` module.

pub const NAME: &str = "typescript";
/// The file a root directory carries when this module owns the repo.
pub const MARKER: &str = "tsconfig.json";
/// The manifest every npm package carries; targets are scoped by it.
pub const MANIFEST: &str = crate::lang::node::MANIFEST;
pub const SOURCE_LABEL: &str = "typescript";
/// The extensions the module treats as source — the ones rust-code-analysis
/// parses. `.mts`/`.cts` are absent because the metrics tool silently skips
/// them, and a file it cannot read is `INCOMPLETE`, not a measurement.
pub const SOURCE_EXTS: [&str; 2] = [".ts", ".tsx"];
/// The binary whose presence means the dev environment is set up.
pub const ENV_TOOL: &str = "node";

/// Whether a path is a typescript source file.
pub fn is_source(path: &str) -> bool {
    SOURCE_EXTS.iter().any(|ext| path.ends_with(ext))
}

/// The embedded baseline, as text — the same document as a repo's `.mido.toml`.
pub const DEFAULTS: &str = include_str!("defaults.toml");

/// The typescript baseline the ladder runs when `.mido.toml` is silent (or absent).
pub fn default_config() -> &'static toml::Table {
    static DEFAULTS_TABLE: std::sync::OnceLock<toml::Table> = std::sync::OnceLock::new();
    DEFAULTS_TABLE.get_or_init(|| {
        DEFAULTS
            .parse()
            .expect("the embedded defaults are valid toml")
    })
}

/// The bare command a gate runs when no config applies: the node ecosystem's
/// commands, plus the typecheck plain javascript has none of.
pub fn fallback_argv(gate: &str, key: &str) -> Option<Vec<String>> {
    let argv = match (gate, key) {
        ("syntax", "typecheck") => &["npx", "--no-install", "tsc", "--noEmit"][..],
        _ => return crate::lang::node::fallback_argv(gate, key),
    };
    Some(argv.iter().map(|arg| (*arg).to_string()).collect())
}

#[cfg(test)]
mod tests;
