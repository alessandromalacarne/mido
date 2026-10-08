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

    /// language module; default: inferred from the repo (a root Cargo.toml selects rust)
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

    /// write the report as markdown to this file
    #[arg(long)]
    pub report: Option<PathBuf>,

    /// print the verdict as json
    #[arg(long)]
    pub json: bool,

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
}

impl LangArg {
    pub fn lang(self) -> Lang {
        match self {
            LangArg::Rust => Lang::Rust,
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
pub fn main_with(
    args: &Args,
    runner: &dyn Runner,
    out: &mut dyn Write,
    err: &mut dyn Write,
    style: Style,
) -> i32 {
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

