//! Language modules.
//!
//! The ladder is generic; everything stack-specific — default commands, output
//! parsers, target detection, the environment probe, the measurement pass —
//! lives in a module under this one. Each module carries its own tools: the
//! rust module measures with tokei and rust-code-analysis, the node modules
//! with eslint. Sharing an integration is a module's choice, never the shape
//! of the seam. The module a run uses is inferred from the repo unless
//! `--lang` names it explicitly.

pub mod javascript;
pub mod node;
pub mod rust;
pub mod typescript;

use crate::error::GuardrailsError;
use std::path::Path;

/// A language module the ladder can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    TypeScript,
    JavaScript,
}

/// Every module this build knows; inference and `--lang` pick from this list,
/// in this order.
pub const LANGS: [Lang; 3] = [Lang::Rust, Lang::TypeScript, Lang::JavaScript];

/// What one syntax step made of its output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepOutcome {
    pub details: Vec<String>,
    pub problems: Vec<String>,
}

/// What a test run's output said, summed over every `test result:` line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestSummary {
    pub passed: i64,
    pub failed: i64,
    pub failed_names: Vec<String>,
}

/// What one mutation run reported: the counts its report files carry, the
/// mutants it left alive, and the exclusion list an `--iterate` pass read at
/// its start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MutationReport {
    pub total: i64,
    pub caught: i64,
    pub missed: i64,
    pub unviable: i64,
    pub timeout: i64,
    pub skipped: i64,
    pub survivors: Vec<String>,
    pub timed_out: Vec<String>,
    pub baseline_failure: Option<String>,
}

impl Lang {
    /// The module the repo itself selects: the first one whose root manifest is
    /// there — a repo carrying both a Cargo.toml and a package.json is rust's,
    /// and a tsconfig.json makes an npm repo typescript's.
    pub fn infer(repo: &Path) -> Result<Self, GuardrailsError> {
        for lang in LANGS {
            if repo.join(lang.manifest()).exists() {
                return Ok(lang);
            }
        }

        Err(
            GuardrailsError::setup("no language module recognizes this repo")
                .detail(format!(
                    "looked for {}",
                    LANGS
                        .iter()
                        .map(|lang| lang.manifest())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .detail(format!(
                    "known languages: {}",
                    LANGS
                        .iter()
                        .map(|lang| lang.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .hint("pass --lang to force a module, or run against a project it knows"),
        )
    }

    /// The module's name, as `--lang` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => rust::NAME,
            Lang::TypeScript => typescript::NAME,
            Lang::JavaScript => javascript::NAME,
        }
    }

    /// The manifest a root directory carries when this module owns the repo.
    pub fn manifest(self) -> &'static str {
        match self {
            Lang::Rust => rust::MANIFEST,
            Lang::TypeScript => typescript::MARKER,
            Lang::JavaScript => javascript::MANIFEST,
        }
    }

    /// The module's built-in contract — the baseline `.mido.toml` applies under
    /// the repo's own file.
    pub fn default_config(self) -> &'static toml::Table {
        match self {
            Lang::Rust => rust::default_config(),
            Lang::TypeScript => typescript::default_config(),
            Lang::JavaScript => javascript::default_config(),
        }
    }

    /// The bare command a gate falls back to when neither the config file nor
    /// the module defaults supply one — the standalone-package case, where root
    /// commands deliberately do not leak in.
    pub fn fallback_argv(self, gate: &str, key: &str) -> Option<Vec<String>> {
        match self {
            Lang::Rust => rust::fallback_argv(gate, key),
            Lang::TypeScript => typescript::fallback_argv(gate, key),
            Lang::JavaScript => node::fallback_argv(gate, key),
        }
    }

    /// Parse one syntax step's output into what the gate reports.
    pub fn syntax_outcome(
        self,
        step: &str,
        command: &str,
        output: &str,
        returncode: i32,
        work_dir: &Path,
    ) -> StepOutcome {
        match self {
            Lang::Rust => {
                rust::diagnostics::syntax_outcome(step, command, output, returncode, work_dir)
            }
            Lang::TypeScript | Lang::JavaScript => {
                node::diagnostics::syntax_outcome(step, command, output, returncode, work_dir)
            }
        }
    }

    /// Fold duplicate diagnostics (two tools reporting the same finding) into
    /// one entry each.
    pub fn dedupe_problems(self, problems: &[String]) -> Vec<String> {
        match self {
            Lang::Rust => rust::diagnostics::dedupe_problems(problems),
            Lang::TypeScript | Lang::JavaScript => node::diagnostics::dedupe_problems(problems),
        }
    }

    /// Measure what the size and analysis gates judge. Every module names its
    /// own tools here; two modules sharing one is a coincidence, not the
    /// shape of the seam.
    pub fn measure(
        self,
        runner: &dyn crate::process::Runner,
        repo: &Path,
        target: &crate::targets::Target,
        files: &[String],
    ) -> crate::metrics::Measurements {
        let probe_tool = self.env_tool();
        match self {
            Lang::Rust => rust::metrics::measure(runner, repo, target, files, probe_tool),
            Lang::TypeScript | Lang::JavaScript => {
                node::eslint::measure(runner, repo, target, files, probe_tool)
            }
        }
    }

    /// The `[size] tool` this module measures with.
    pub fn size_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::SIZE_TOOL,
            Lang::TypeScript | Lang::JavaScript => node::eslint::TOOL,
        }
    }

    /// The `[analysis] tool` this module measures with.
    pub fn metrics_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::METRICS_TOOL,
            Lang::TypeScript | Lang::JavaScript => node::eslint::TOOL,
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
            Lang::TypeScript | Lang::JavaScript => node::suite::test_commands(config, target, repo),
        }
    }

    /// Parse a test command's output; `None` when it printed no summary.
    pub fn test_summary(self, output: &str) -> Option<TestSummary> {
        match self {
            Lang::Rust => rust::suite::test_summary(output),
            Lang::TypeScript | Lang::JavaScript => node::suite::test_summary(output),
        }
    }

    /// The coverage command this module runs for a target: the config's own
    /// first, then what the module derives for the repo, then its bare
    /// fallback.
    pub fn coverage_command(
        self,
        config: &crate::config::Config,
        target: &crate::targets::Target,
        repo: &Path,
    ) -> Option<Vec<String>> {
        match self {
            Lang::Rust => config
                .argv("coverage", "command", target)
                .or_else(|| rust::fallback_argv("coverage", "command")),
            Lang::TypeScript | Lang::JavaScript => node::coverage::command(config, target, repo),
        }
    }

    /// Where the coverage command's own line says its report lands; `None`
    /// when it names no path, and the gate's `lcov.info` default applies.
    pub fn coverage_report(self, argv: &[String]) -> Option<std::path::PathBuf> {
        match self {
            Lang::Rust => rust::coverage::report_path(argv),
            Lang::TypeScript | Lang::JavaScript => node::coverage::report_path(argv),
        }
    }

    /// The label the banner and reports use for this module's source files.
    pub fn source_label(self) -> &'static str {
        match self {
            Lang::Rust => rust::SOURCE_LABEL,
            Lang::TypeScript => typescript::SOURCE_LABEL,
            Lang::JavaScript => javascript::SOURCE_LABEL,
        }
    }

    /// Whether a path is a source file of this module.
    pub fn is_source(self, path: &str) -> bool {
        match self {
            Lang::Rust => rust::is_source(path),
            Lang::TypeScript => typescript::is_source(path),
            Lang::JavaScript => javascript::is_source(path),
        }
    }

    /// The binary whose presence means the dev environment is set up.
    pub fn env_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::ENV_TOOL,
            Lang::TypeScript => typescript::ENV_TOOL,
            Lang::JavaScript => javascript::ENV_TOOL,
        }
    }

    /// The mutation tool's name, as reports spell it.
    pub fn mutation_tool(self) -> &'static str {
        match self {
            Lang::Rust => rust::mutation::TOOL,
            Lang::TypeScript | Lang::JavaScript => node::mutation::TOOL,
        }
    }

    /// Write whatever state the module's mutation tool needs under the run's
    /// own output directory, before the passes run.
    pub fn mutation_prepare(self, output: &Path, work_dir: &Path) -> Result<(), GuardrailsError> {
        match self {
            Lang::Rust => Ok(()),
            Lang::TypeScript | Lang::JavaScript => node::mutation::prepare(output, work_dir),
        }
    }

    /// One mutation pass's argv: the configured command plus what the module's
    /// mutation tool needs to run under mido's own output directory, carrying
    /// the test arguments of the suite the pass drives and, when the run is
    /// scoped, the change diff. A command that names its own output — or its
    /// own scope under a scoped run — is a conflict the module refuses.
    pub fn mutation_pass_args(
        self,
        base: &[String],
        test_args: &[String],
        iterate: bool,
        output: &Path,
        in_diff: Option<&Path>,
    ) -> Result<Vec<String>, GuardrailsError> {
        match self {
            Lang::Rust => rust::mutation::pass_args(base, test_args, iterate, output, in_diff),
            Lang::TypeScript | Lang::JavaScript => {
                node::mutation::pass_args(base, test_args, iterate, output, in_diff)
            }
        }
    }

    /// The report a mutation run wrote into its output directory; `None` when
    /// it wrote none, or died before finishing.
    pub fn mutation_report(self, output_dir: &Path) -> Option<MutationReport> {
        match self {
            Lang::Rust => rust::mutation::report::read(output_dir),
            Lang::TypeScript | Lang::JavaScript => node::mutation::report::read(output_dir),
        }
    }

    /// Whether a mutation run's output says it found nothing to filter in the
    /// scope it was given.
    pub fn mutation_found_nothing(self, output: &str) -> bool {
        match self {
            Lang::Rust => rust::mutation::found_nothing(output),
            // Stryker prints no such signal, and the change scope is not wired
            // for the node modules yet: their sweep is the whole package.
            Lang::TypeScript | Lang::JavaScript => false,
        }
    }

    /// The arguments a declared test command carries to the test tool; `None`
    /// when the command names none, so the tool's own default applies.
    pub fn mutation_test_args(self, command: &[String]) -> Option<Vec<String>> {
        match self {
            Lang::Rust => rust::mutation::test_args(command),
            Lang::TypeScript | Lang::JavaScript => node::mutation::test_args(command),
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
            Lang::TypeScript | Lang::JavaScript => node::targets::detect_targets(repo, config),
        }
    }

    /// Resolve a `-p` package name as this module understands it.
    pub fn resolve_package(
        self,
        repo: &Path,
        config: &crate::config::Config,
        spec: &str,
    ) -> Result<crate::targets::Target, GuardrailsError> {
        match self {
            Lang::Rust => rust::targets::resolve_package(repo, config, spec),
            Lang::TypeScript | Lang::JavaScript => {
                node::targets::resolve_package(repo, config, spec)
            }
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
            Lang::TypeScript | Lang::JavaScript => ensure_manifest(repo, target),
        }
    }

    /// The error a run gets when no target of this module is configured to exist.
    pub fn no_targets_error(self, repo: &Path) -> GuardrailsError {
        match self {
            Lang::Rust => rust::no_targets_error(repo),
            Lang::TypeScript | Lang::JavaScript => node::no_targets_error(repo),
        }
    }
}

/// The manifest a target names, or the setup error a run gets when it is gone —
/// one check every module's target validation starts with.
pub(crate) fn ensure_manifest(
    repo: &Path,
    target: &crate::targets::Target,
) -> Result<(), GuardrailsError> {
    let Some(manifest) = &target.manifest else {
        return Ok(());
    };
    let path = repo.join(manifest);
    if path.exists() {
        return Ok(());
    }

    Err(
        GuardrailsError::setup(format!("target `{}` has no manifest", target.name))
            .detail(format!("looked for {}", path.display()))
            .hint("pass --list-targets to see what can be measured"),
    )
}

#[cfg(test)]
mod tests;
