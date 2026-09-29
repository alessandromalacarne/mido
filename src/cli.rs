//! The command line the ladder is driven with.

use crate::error::RunError;
use crate::process::Runner;
use crate::session::{run_session, Io};
use crate::style::Style;
use clap::{Parser, ValueEnum};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "mido",
    version,
    about = "Run the guardrails ladder (`.guardrails.toml`) against one target of the repo.",
    long_about = "Run the guardrails ladder (`.guardrails.toml`) against one target of the repo.\n\n\
                  Exit codes: 0 SHIP-READY, 1 BLOCKED, 2 INCOMPLETE."
)]
pub struct Args {
    /// target name or path; default `auto` (inferred from the diff)
    #[arg(default_value = "auto")]
    pub target: String,

    /// repository (or worktree) root; defaults to cwd
    #[arg(long)]
    pub repo: Option<PathBuf>,

    /// ref the changed files are computed against
    #[arg(long)]
    pub base: Option<String>,

    /// run only this gate (repeatable)
    #[arg(long = "gate")]
    pub gates: Vec<Gate>,

    /// run every target in turn
    #[arg(long)]
    pub all: bool,

    /// print the detected targets and exit
    #[arg(long = "list-targets")]
    pub list_targets: bool,

    /// add the local [workspace] aid if missing
    #[arg(long = "apply-workspace-aid")]
    pub apply_workspace_aid: bool,

    /// lcov from the base revision, for the delta
    #[arg(long = "baseline-lcov")]
    pub baseline_lcov: Option<PathBuf>,

    /// write the report as markdown to this file
    #[arg(long)]
    pub report: Option<PathBuf>,

    /// print the verdict as json
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Gate {
    Syntax,
    Size,
    Analysis,
    Tests,
    Coverage,
    Mutation,
}

impl Gate {
    pub fn name(self) -> &'static str {
        match self {
            Gate::Syntax => "syntax",
            Gate::Size => "size",
            Gate::Analysis => "analysis",
            Gate::Tests => "tests",
            Gate::Coverage => "coverage",
            Gate::Mutation => "mutation",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_target_is_auto() {
        assert_eq!(parse_from(&[]).target, "auto");
    }

    #[test]
    fn every_gate_name_is_selectable() {
        let args = parse_from(&["--gate", "coverage"]);

        assert_eq!(args.gates, vec![Gate::Coverage]);
    }

    #[test]
    fn flags_argv() {
        let args = parse_from(&[
            "--repo",
            "/tmp/x",
            "frontend",
            "--base",
            "origin/mvp",
            "--json",
            "--all",
        ]);

        assert_eq!(args.repo, Some(PathBuf::from("/tmp/x")));
        assert_eq!(args.target, "frontend");
        assert_eq!(args.base.as_deref(), Some("origin/mvp"));
        assert!(args.json && args.all);
    }
}
