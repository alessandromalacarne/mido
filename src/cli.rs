//! The command line the ladder is driven with.

use crate::error::RunError;
use crate::lang::Lang;
use crate::process::Runner;
use crate::session::{run_session, Io};
use crate::style::Style;
use clap::{Parser, Subcommand, ValueEnum};
use std::io::Write;
use std::path::PathBuf;

pub use crate::gate::Gate;

/// Where a bare `--report` writes: the repo's own file, next to `.mido.toml`.
pub const DEFAULT_REPORT: &str = ".mido/report.md";

#[derive(Debug, Clone, Parser)]
#[command(
    name = "mido",
    version,
    about = "Run the guardrails ladder (`.mido.toml`) against the workspace, or the packages `-p` names.",
    long_about = "Run the guardrails ladder (`.mido.toml`) against the workspace, or the packages `-p` names.\n\n\
                  Every gate measures the whole target: no diff is read.\n\n\
                  Exit codes: 0 SHIP-READY, 1 BLOCKED, 2 INCOMPLETE.",
    subcommand_precedence_over_arg = true
)]
pub struct Args {
    /// package to measure, by cargo package name (repeatable); default: the whole workspace
    #[arg(short = 'p', long = "package", value_name = "NAME")]
    pub packages: Vec<String>,

    /// language module; default: inferred from the repo (a root Cargo.toml
    /// selects rust, a root tsconfig.json selects typescript, a root
    /// package.json selects javascript)
    #[arg(long)]
    pub lang: Option<LangArg>,

    /// repository (or worktree) root; defaults to cwd
    #[arg(long)]
    pub repo: Option<PathBuf>,

    /// run only this gate (repeatable)
    #[arg(long = "gate")]
    pub gates: Vec<Gate>,

    /// print the detected targets and exit
    #[arg(long = "list-targets")]
    pub list_targets: bool,

    /// add the local [workspace] aid if missing
    #[arg(long = "apply-workspace-aid")]
    pub apply_workspace_aid: bool,

    /// lcov to compare totals against, for the delta
    #[arg(long = "baseline-lcov")]
    pub baseline_lcov: Option<PathBuf>,

    /// write the report as markdown to this file; without a path: the repo's own .mido/report.md
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = DEFAULT_REPORT)]
    pub report: Option<PathBuf>,

    /// print the verdict as json
    #[arg(long)]
    pub json: bool,

    /// print nothing on stdout; the exit code is the verdict, the report is the evidence
    #[arg(long)]
    pub silent: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What `mido` can be asked to do besides measuring a target.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// serve the ladder as MCP tools on stdio, for an LLM agent to call
    Mcp,
}

/// The language modules `--lang` can force, overriding inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LangArg {
    Rust,
    Typescript,
    Javascript,
}

impl LangArg {
    pub fn lang(self) -> Lang {
        match self {
            LangArg::Rust => Lang::Rust,
            LangArg::Typescript => Lang::TypeScript,
            LangArg::Javascript => Lang::JavaScript,
        }
    }
}

pub fn parse() -> Args {
    Args::parse()
}

pub fn parse_from(argv: &[&str]) -> Args {
    Args::parse_from(std::iter::once("mido").chain(argv.iter().copied()))
}

/// Parsing that reports instead of exiting, for callers that need the reason.
pub fn try_parse_from(argv: &[&str]) -> Result<Args, clap::Error> {
    Args::try_parse_from(std::iter::once("mido").chain(argv.iter().copied()))
}

/// Run the ladder and turn its outcome into a process exit code.
///
/// `--silent` swaps stdout for a sink — every print goes through the `Io`, so
/// one swap silences the whole run; the exit code stays the verdict and stderr
/// keeps its errors and warnings.
pub fn main_with(
    args: &Args,
    runner: &dyn Runner,
    out: &mut dyn Write,
    err: &mut dyn Write,
    style: Style,
) -> i32 {
    let mut sink = std::io::sink();
    let out: &mut dyn Write = if args.silent { &mut sink } else { out };
    let mut io = Io { out, err, style };
    match run_session(args, runner, &mut io) {
        Ok(code) => code,
        Err(RunError::Error(error)) => {
            let _ = writeln!(io.err, "{}", error.render_styled(io.style));
            error.exit_code()
        }
        Err(RunError::Failure(failure)) => {
            let _ = writeln!(
                io.err,
                "{} guardrails {}",
                io.style.fail("error:"),
                failure.verdict
            );
            failure.exit_code()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_packages_the_whole_workspace_is_measured() {
        assert!(parse_from(&[]).packages.is_empty());
    }

    #[test]
    fn the_package_flag_is_repeatable_and_has_a_short_form() {
        assert_eq!(
            parse_from(&["-p", "lib", "--package", "api"]).packages,
            vec!["lib", "api"]
        );
    }

    #[test]
    fn the_language_flag_selects_a_module() {
        assert_eq!(parse_from(&["--lang", "rust"]).lang, Some(LangArg::Rust));
        assert_eq!(
            parse_from(&["--lang", "typescript"]).lang,
            Some(LangArg::Typescript)
        );
        assert_eq!(
            parse_from(&["--lang", "javascript"]).lang,
            Some(LangArg::Javascript)
        );
    }

    #[test]
    fn an_unknown_language_is_never_silently_accepted() {
        let rejected = try_parse_from(&["--lang", "python"]);

        assert!(rejected.is_err(), "an unknown module is not a module");
        assert_eq!(rejected.expect_err("rejected").exit_code(), 2);
    }

    #[test]
    fn every_gate_name_is_selectable() {
        let args = parse_from(&["--gate", "coverage"]);

        assert_eq!(args.gates, vec![Gate::Coverage]);
    }

    #[test]
    fn a_positional_target_is_not_a_target_anymore() {
        let rejected = try_parse_from(&["frontend"]);

        assert!(rejected.is_err(), "a bare positional is a usage error");
        assert_eq!(rejected.expect_err("rejected").exit_code(), 2);
    }

    #[test]
    fn the_removed_scope_flags_are_not_accepted() {
        for flag in ["--base", "--path", "--no-diff", "--all"] {
            let rejected = try_parse_from(&[flag, "x"]);
            assert!(rejected.is_err(), "`{flag}` is gone");
        }
    }

    #[test]
    fn flags_argv() {
        let args = parse_from(&["--repo", "/tmp/x", "-p", "frontend", "--json"]);

        assert_eq!(args.repo, Some(PathBuf::from("/tmp/x")));
        assert_eq!(args.packages, vec!["frontend"]);
        assert!(args.json);
    }

    #[test]
    fn the_silent_flag_asks_for_no_stdout() {
        let silent = try_parse_from(&["--silent"]).expect("--silent is a flag");
        let loud = try_parse_from(&[]).expect("no flag is valid");

        assert!(silent.silent);
        assert!(!loud.silent);
    }

    #[test]
    fn a_bare_report_flag_asks_for_the_repos_own_report() {
        let args = try_parse_from(&["--report"]).expect("a bare --report is valid");

        assert_eq!(args.report, Some(PathBuf::from(DEFAULT_REPORT)));
    }

    #[test]
    fn a_report_flag_with_a_path_keeps_the_path() {
        assert_eq!(
            parse_from(&["--report", "out.md"]).report,
            Some(PathBuf::from("out.md"))
        );
    }

    #[test]
    fn a_bare_report_flag_does_not_swallow_the_next_flag() {
        let args = parse_from(&["--report", "--json"]);

        assert_eq!(args.report, Some(PathBuf::from(DEFAULT_REPORT)));
        assert!(args.json);
    }

    #[test]
    fn the_mcp_subcommand_is_recognized() {
        assert_eq!(parse_from(&["mcp"]).command, Some(Command::Mcp));
    }
}
