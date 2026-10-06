//! Language modules.
//!
//! The ladder is generic; everything stack-specific — default commands, output
//! parsers, target detection, the environment probe — lives in a module under
//! this one. The module a run uses is inferred from the repo unless `--lang`
//! names it explicitly.

pub mod rust;

use crate::error::GuardrailsError;
use std::path::Path;

/// A language module the ladder can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
}

/// Every module this build knows; inference and `--lang` pick from this list.
pub const LANGS: [Lang; 1] = [Lang::Rust];

/// What one syntax step made of its output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepOutcome {
    pub details: Vec<String>,
    pub problems: Vec<String>,
    pub crate_wide_debt: bool,
}

/// What a test run's output said, summed over every `test result:` line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestSummary {
    pub passed: i64,
    pub failed: i64,
    pub failed_names: Vec<String>,
}

/// The counts a mutation run reported.
///
/// `skipped` are mutants an `--iterate` run excluded as previously caught or
/// unviable; they are not counted in `total`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MutationSummary {
    pub total: i64,
    pub caught: i64,
    pub missed: i64,
    pub unviable: i64,
    pub skipped: i64,
}

/// How a mutation run is scoped.
pub struct MutationScope<'a> {
    pub configured: &'a str,
    pub scope: crate::targets::Scope,
    pub changed: &'a [String],
    pub patch: &'a Path,
}

impl Lang {
    /// The module the repo itself selects: a root manifest names one.
    pub fn infer(repo: &Path) -> Result<Self, GuardrailsError> {
        let manifest = repo.join(rust::MANIFEST);
        if manifest.exists() {
            return Ok(Lang::Rust);
        }

        Err(
            GuardrailsError::setup("no language module recognizes this repo")
                .detail(format!("looked for {}", manifest.display()))
                .detail(format!(
                    "known languages: {}",
                    LANGS
                        .iter()
                        .map(|lang| lang.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .hint("pass --lang rust to force a module, or run against a project it knows"),
        )
    }

    /// The module's name, as `--lang` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => rust::NAME,
        }
    }

    /// The module's built-in contract — the baseline `.mido.toml` applies under
    /// the repo's own file.
    pub fn default_config(self) -> &'static toml::Table {
        match self {
            Lang::Rust => rust::default_config(),
        }
    }

    /// The bare command a gate falls back to when neither the config file nor
    /// the module defaults supply one — the standalone-crate case, where root
    /// commands deliberately do not leak in.
    pub fn fallback_argv(self, gate: &str, key: &str) -> Option<Vec<String>> {
        match self {
            Lang::Rust => rust::fallback_argv(gate, key),
        }
    }

    /// Parse one syntax step's output into what the gate reports.
    pub fn syntax_outcome(
        self,
        step: &str,
        command: &str,
        output: &str,
        returncode: i32,
        changed: &[String],
    ) -> StepOutcome {
        match self {
            Lang::Rust => {
                rust::diagnostics::syntax_outcome(step, command, output, returncode, changed)
            }
        }
    }

    /// Fold duplicate diagnostics (clippy and `cargo check` report the same
    /// lint at the same place) into one entry each.
    pub fn dedupe_problems(self, problems: &[String]) -> Vec<String> {
        match self {
            Lang::Rust => rust::diagnostics::dedupe_problems(problems),
        }
    }

    /// Measure the function metrics the size and analysis gates judge.
    pub fn analysis_units(
        self,
        runner: &dyn crate::process::Runner,
        repo: &Path,
        target: &crate::targets::Target,
        files: &[String],
        on_error: &mut Vec<String>,
    ) -> Vec<crate::metrics::Unit> {
        match self {
            Lang::Rust => rust::analysis::analysis_units(runner, repo, target, files, on_error),
        }
    }

    /// The code lines per file the size gate judges.
    pub fn code_lines(
        self,
        runner: &dyn crate::process::Runner,
        repo: &Path,
        target: &crate::targets::Target,
        files: &[String],
        tool: &str,
        on_error: &mut Vec<String>,
    ) -> std::collections::BTreeMap<String, i64> {
        match self {
            Lang::Rust => rust::size::code_lines(runner, repo, target, files, tool, on_error),
        }
    }

    /// The `[size] tool` this module drives by default.
    pub fn size_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::size::TOOL,
        }
    }

    /// The tool that produces the function metrics both size and analysis judge.
    pub fn metrics_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::METRICS_TOOL,
        }
    }

    /// The commands the tests gate runs for a target, in order.
    pub fn test_commands(
        self,
        config: &crate::config::Config,
        target: &crate::targets::Target,
        repo: &Path,
    ) -> Vec<Vec<String>> {
        match self {
            Lang::Rust => rust::suite::test_commands(config, target, repo),
        }
    }

    /// Parse a test command's output; `None` when it printed no summary.
    pub fn test_summary(self, output: &str) -> Option<TestSummary> {
        match self {
            Lang::Rust => rust::suite::test_summary(output),
        }
    }

    /// The label the banner and reports use for this module's source files.
    pub fn source_label(self) -> &'static str {
        match self {
            Lang::Rust => rust::SOURCE_LABEL,
        }
    }

    /// Whether a path is a source file of this module.
    pub fn is_source(self, path: &str) -> bool {
        match self {
            Lang::Rust => rust::is_source(path),
        }
    }

    /// The binary whose presence means the dev environment is set up.
    pub fn env_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::ENV_TOOL,
        }
    }

    /// The mutation tool's name, as reports spell it.
    pub fn mutation_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::mutation::TOOL,
        }
    }

    /// The per-mutant timeout argument value.
    pub fn mutation_timeout(self) -> i64 {
        match self {
            Lang::Rust => rust::mutation::TIMEOUT_SECS,
        }
    }

    /// Parse a mutation run's output; `None` when it printed no summary.
    pub fn mutation_summary(self, output: &str) -> Option<MutationSummary> {
        match self {
            Lang::Rust => rust::mutation::mutation_summary(output),
        }
    }

    /// The arguments that scope a mutation run to the config's `scope` setting.
    pub fn mutation_scope_args(
        self,
        runner: &dyn crate::process::Runner,
        repo: &Path,
        target: &crate::targets::Target,
        scoped: MutationScope<'_>,
    ) -> Result<Vec<String>, GuardrailsError> {
        match self {
            Lang::Rust => rust::mutation::scope_args(runner, repo, target, scoped),
        }
    }

    /// Every target this module can measure in the repo.
    pub fn detect_targets(
        self,
        repo: &Path,
        config: &crate::config::Config,
    ) -> std::collections::BTreeMap<String, crate::targets::Target> {
        match self {
            Lang::Rust => rust::targets::detect_targets(repo, config),
        }
    }

    /// Resolve a target name or path as this module understands it.
    pub fn resolve_target(
        self,
        repo: &Path,
        config: &crate::config::Config,
        spec: &str,
    ) -> Result<crate::targets::Target, GuardrailsError> {
        match self {
            Lang::Rust => rust::targets::resolve_target(repo, config, spec),
        }
    }

    /// Check the target's manifest is there and resolves where it should.
    pub fn validate_target_setup(
        self,
        runner: &dyn crate::process::Runner,
        repo: &Path,
        target: &crate::targets::Target,
        apply_aid: bool,
        out: &mut dyn std::io::Write,
    ) -> Result<(), GuardrailsError> {
        match self {
            Lang::Rust => rust::aid::validate_target_setup(runner, repo, target, apply_aid, out),
        }
    }

    /// The error a run gets when no target of this module is configured to exist.
    pub fn no_targets_error(self, repo: &Path) -> GuardrailsError {
        match self {
            Lang::Rust => rust::no_targets_error(repo),
        }
    }
}

#[cfg(test)]
mod tests;
