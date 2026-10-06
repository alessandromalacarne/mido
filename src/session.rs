//! One run of the ladder: what it measures, in what order, and what it prints.

pub mod output;
pub mod setup;

pub use output::{markdown_report, print_targets, print_verdict, write_report};
pub use setup::{build_session, select_targets};

use crate::cli::Args;
use crate::config::Config;
use crate::error::{GateFailure, GuardrailsError, RunError};
use crate::gate::Gate;
use crate::gates::{run_gates, GateRun};
use crate::lang::Lang;
use crate::process::{self, Runner};
use crate::report::{exit_code, render_failure, verdict, FailureContext, GateResult};
use crate::style::Style;
use crate::targets::{scope_changed, Scope, Target};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    pub style: Style,
}

/// Everything a ladder run measures, resolved once.
pub struct Session {
    pub repo: PathBuf,
    pub config: Config,
    pub lang: Lang,
    pub scope: Scope,
    pub base: String,
    pub changed: Vec<String>,
    pub revision: String,
    pub dirty: String,
    pub gates: Vec<Gate>,
    pub scratch: PathBuf,
    pub baseline_lcov: Option<PathBuf>,
    pub report_path: Option<PathBuf>,
    pub apply_aid: bool,
    pub as_json: bool,
    pub selection: String,
}

pub fn scratch_dir() -> PathBuf {
    scratch_dir_in(sandbox_root().unwrap_or_else(cache_root))
}

/// The session's artifact directory, created so the caller can write into it.
fn scratch_dir_in(base: PathBuf) -> PathBuf {
    let directory = base.join("guardrails");
    let _ = std::fs::create_dir_all(&directory);
    directory
}

/// `$COMMANDCODE_SCRATCHPAD` when the session set one, else the home cache.
fn sandbox_root() -> Option<PathBuf> {
    std::env::var("COMMANDCODE_SCRATCHPAD")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn cache_root() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".cache")
}

pub fn default_report_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    // The guardrails report is the handoff artifact `/walkthrough` and `/lgtm`
    // look for; write it where the session keeps its artifacts.
    explicit.or_else(|| sandbox_root().map(|root| root.join("guardrails-report.md")))
}

/// Where this run writes its report: an explicit path resolved against the repo,
/// or the session scratchpad. Never a path relative to wherever the process happens
/// to stand — a report is an artifact, not a surprise in the working tree.
pub fn report_path(explicit: Option<PathBuf>, repo: &Path) -> Option<PathBuf> {
    match default_report_path(explicit) {
        Some(path) if path.is_absolute() => Some(path),
        Some(path) => Some(repo.join(path)),
        None => None,
    }
}

pub fn run_session(args: &Args, runner: &dyn Runner, io: &mut Io<'_>) -> Result<i32, RunError> {
    let repo = open_repo(args, runner);
    let lang = resolve_lang(args, &repo)?;
    let config = Config::load(&repo, &lang)?;
    announce_warnings(&config, io);

    let targets = lang.detect_targets(&repo, &config);
    if args.list_targets {
        print_targets(io.out, &repo, &targets, io.style);
        return Ok(0);
    }
    if targets.is_empty() {
        return Err(RunError::Error(lang.no_targets_error(&repo)));
    }

    let session = build_session(args, &repo, config, runner, lang)?;
    measure_selected(runner, args, &session, &targets, io)
}

/// The repo the run measures: the one `--repo` names, canonicalized, or the cwd.
fn open_repo(args: &Args, runner: &dyn Runner) -> PathBuf {
    let cwd = args
        .repo
        .clone()
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    process::workspace_root(runner, &cwd)
}

/// The selected targets, measured in turn; the exit code the run earns.
fn measure_selected(
    runner: &dyn Runner,
    args: &Args,
    session: &Session,
    targets: &BTreeMap<String, Target>,
    io: &mut Io<'_>,
) -> Result<i32, RunError> {
    let Some(selected) = select_targets(args, session, targets, io.out, io.style)? else {
        return Ok(2);
    };

    let (measured, reports) = measure(runner, session, &selected, io)?;
    if !measured {
        report_no_gates(io);
        return Ok(2);
    }

    write_report(session.report_path.as_deref(), &reports, io.out, io.style);
    Ok(0)
}

/// The language module this run uses: the one `--lang` names, or the one the
/// repo itself selects.
fn resolve_lang(args: &Args, repo: &Path) -> Result<Lang, RunError> {
    match args.lang {
        Some(arg) => Ok(arg.lang()),
        None => Ok(Lang::infer(repo)?),
    }
}

fn announce_warnings(config: &Config, io: &mut Io<'_>) {
    for warning in config.warnings() {
        let _ = writeln!(io.err, "{} {warning}", io.style.warn("warning:"));
    }
}

/// Every selected target, in turn; the first blocked one ends the run.
fn measure(
    runner: &dyn Runner,
    session: &Session,
    selected: &[Target],
    io: &mut Io<'_>,
) -> Result<(bool, Vec<String>), RunError> {
    let mut reports: Vec<String> = Vec::new();
    let mut measured = false;

    for target in selected {
        let Some(results) = run_target(runner, session, target, io)? else {
            continue;
        };

        measured = true;
        print_verdict(io.out, session, target, &results, io.style);
        reports.push(markdown_report(session, target, &results));

        if exit_code(&results) != 0 {
            return Err(raise_blocked(session, target, &results, &reports, io));
        }
    }

    Ok((measured, reports))
}

/// The gates for one target; `None` when the target owns none of the diff.
pub fn run_target(
    runner: &dyn Runner,
    session: &Session,
    target: &Target,
    io: &mut Io<'_>,
) -> Result<Option<Vec<GateResult>>, GuardrailsError> {
    let scoped = scope_changed(&session.changed, target);
    let _ = writeln!(
        io.out,
        "{}",
        output::target_banner(session, target, &scoped, io.style)
    );
    let _ = writeln!(io.out);

    if scoped.is_empty() {
        let _ = writeln!(io.out, "{}\n", output::skip_note(target, io.style));
        return Ok(None);
    }

    session
        .lang
        .validate_target_setup(runner, &session.repo, target, session.apply_aid, io.out)?;

    let results = run_gates(
        runner,
        &gate_run(session, target, &scoped),
        io.out,
        io.style,
    );
    Ok(Some(results))
}

/// The contract between the orchestration and the gates for one target.
fn gate_run<'a>(session: &'a Session, target: &'a Target, scoped: &'a [String]) -> GateRun<'a> {
    GateRun {
        repo: &session.repo,
        target,
        config: &session.config,
        lang: session.lang,
        scope: session.scope,
        changed: scoped,
        gates: &session.gates,
        scratch: &session.scratch,
        baseline_lcov: session.baseline_lcov.as_deref(),
    }
}

/// Print the detailed failure, persist the evidence, then block the run.
fn raise_blocked(
    session: &Session,
    target: &Target,
    results: &[GateResult],
    reports: &[String],
    io: &mut Io<'_>,
) -> RunError {
    let attempts = session.config.attempts_cap();
    let final_verdict = verdict(results);
    let failure = render_failure(
        results,
        &FailureContext {
            target: &target.label(),
            revision: &session.revision,
            dirty: &session.dirty,
            base: &session.base,
            attempts,
        },
        io.style,
    );

    let _ = writeln!(io.out, "{failure}");
    write_report(session.report_path.as_deref(), reports, io.out, io.style);
    // The exit code is the ladder's own reading of the results: 1 when a gate
    // failed, 2 when none could run to a verdict.
    RunError::Failure(GateFailure::new(failure, final_verdict, exit_code(results)))
}

/// Nothing ran, so nothing passed: exit 2, never a verdict.
fn report_no_gates(io: &mut Io<'_>) {
    let _ = writeln!(
        io.out,
        "no gate ran: no changed file belongs to any of the selected target(s)."
    );
    let _ = writeln!(
        io.out,
        "{}",
        io.style
            .dim("A gate that did not run is not a gate that passed — nothing here is ship-ready.")
    );
}

#[cfg(test)]
mod tests;
