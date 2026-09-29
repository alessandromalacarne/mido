//! One run of the ladder: what it measures, in what order, and what it prints.

use crate::aid::validate_target_setup;
use crate::cli::Args;
use crate::config::Config;
use crate::error::{GateFailure, GuardrailsError, RunError};
use crate::gates::{run_gates, GateRun};
use crate::process::{self, Runner};
use crate::report::{
    exit_code, render_banner, render_failure, render_report_markdown, verdict, FailureContext,
    GateResult, ReportContext, GATES, MAX_BANNER_FILES,
};
use crate::targets::{detect_targets, pick_auto_target, resolve_target, scope_changed, Target};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Everything a ladder run measures, resolved once.
pub struct Session {
    pub repo: PathBuf,
    pub config: Config,
    pub base: String,
    pub changed: Vec<String>,
    pub revision: String,
    pub dirty: String,
    pub gates: Vec<String>,
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

pub fn build_session(args: &Args, repo: &Path, config: Config, runner: &dyn Runner) -> Session {
    let base = args
        .base
        .clone()
        .unwrap_or_else(|| process::pick_base(runner, repo));
    Session {
        repo: repo.to_path_buf(),
        config,
        base: base.clone(),
        changed: process::changed_files(runner, repo, &base),
        revision: process::git(runner, repo, &["rev-parse", "HEAD"])
            .trim()
            .to_string(),
        dirty: process::dirty_hash(runner, repo),
        gates: if args.gates.is_empty() {
            GATES.iter().map(|gate| (*gate).to_string()).collect()
        } else {
            args.gates
                .iter()
                .map(|gate| gate.name().to_string())
                .collect()
        },
        scratch: scratch_dir(),
        baseline_lcov: args.baseline_lcov.clone(),
        report_path: report_path(args.report.clone(), repo),
        apply_aid: args.apply_workspace_aid,
        as_json: args.json,
        selection: if args.target == "auto" {
            "auto"
        } else {
            "requested"
        }
        .to_string(),
    }
}

pub fn print_targets(out: &mut dyn Write, repo: &Path, targets: &BTreeMap<String, Target>) {
    let _ = writeln!(out, "targets under {}:", repo.display());
    for (name, target) in targets {
        let kind = if target.path.is_empty() {
            "workspace"
        } else if target.workspace_member {
            "member"
        } else {
            "standalone crate"
        };
        let path = if target.path.is_empty() {
            "."
        } else {
            target.path.as_str()
        };
        let _ = writeln!(out, "  {name:<12} {path:<16} {kind}");
    }
}

/// Which targets this run measures; `None` means "nothing to measure here".
pub fn select_targets(
    args: &Args,
    session: &Session,
    targets: &BTreeMap<String, Target>,
    out: &mut dyn Write,
) -> Result<Option<Vec<Target>>, GuardrailsError> {
    // Opening a named target is checked first: a typo is a typo whether or not
    // the tree happens to have changes.
    let requested = if args.all || args.target == "auto" {
        None
    } else {
        Some(resolve_target(
            &session.repo,
            &session.config,
            &args.target,
        )?)
    };

    if session.changed.is_empty() {
        let _ = writeln!(
            out,
            "nothing changed against {} — no revision to measure.\nPass --base <sha before the change>, or check out a branch with the work.",
            session.base
        );
        return Ok(None);
    }

    if let Some(target) = requested {
        return Ok(Some(vec![target]));
    }

    if args.all {
        return Ok(Some(
            targets
                .values()
                .filter(|target| !target.path.is_empty() || target.workspace_member)
                .cloned()
                .collect(),
        ));
    }

    match pick_auto_target(targets, &session.changed)? {
        Some(inferred) => Ok(Some(vec![inferred])),
        None => {
            let listing: Vec<String> = session
                .changed
                .iter()
                .take(MAX_BANNER_FILES)
                .map(|path| format!("  {path}"))
                .collect();
            let _ = writeln!(
                out,
                "nothing to measure: no changed file belongs to a cargo target.\n{}\nName a target explicitly (--list-targets) if the ladder should run anyway.",
                listing.join("\n")
            );
            Ok(None)
        }
    }
}

pub fn markdown_report(session: &Session, target: &Target, results: &[GateResult]) -> String {
    render_report_markdown(
        results,
        &ReportContext {
            target,
            base: &session.base,
            revision: &session.revision,
            dirty: &session.dirty,
            changed: &scope_changed(&session.changed, target),
            runner: session.config.script().as_deref().unwrap_or_default(),
        },
    )
}

pub fn print_verdict(
    out: &mut dyn Write,
    session: &Session,
    target: &Target,
    results: &[GateResult],
) {
    let final_verdict = verdict(results);
    let _ = writeln!(out, "verdict: {final_verdict}");
    let _ = writeln!(
        out,
        "revision stamp: {} | {}",
        session.revision, session.dirty
    );
    if !session.as_json {
        return;
    }

    let gates: Vec<serde_json::Value> = results
        .iter()
        .map(|result| {
            serde_json::json!({
                "gate": result.name,
                "status": result.status,
                "summary": result.summary,
            })
        })
        .collect();
    let _ = writeln!(
        out,
        "{}",
        serde_json::json!({
            "target": target.name,
            "verdict": final_verdict,
            "revision": session.revision,
            "dirty": session.dirty,
            "gates": gates,
        })
    );
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
        render_banner(
            target,
            &session.base,
            &session.revision,
            &session.dirty,
            &scoped,
            &session.selection,
            session.config.script().as_deref().unwrap_or_default(),
        )
    );
    let _ = writeln!(io.out);

    if scoped.is_empty() {
        let _ = writeln!(
            io.out,
            "no changed file belongs to {} — skipping.\n",
            target.label()
        );
        return Ok(None);
    }

    validate_target_setup(runner, &session.repo, target, session.apply_aid, io.out)?;

    let results = run_gates(
        runner,
        &GateRun {
            repo: &session.repo,
            target,
            config: &session.config,
            changed: &scoped,
            gates: &session.gates,
            scratch: &session.scratch,
            baseline_lcov: session.baseline_lcov.as_deref(),
        },
        io.out,
    );
    Ok(Some(results))
}

pub fn write_report(report_path: Option<&Path>, reports: &[String], out: &mut dyn Write) {
    let Some(report_path) = report_path else {
        return;
    };
    if let Some(parent) = report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(report_path, reports.join("\n"));
    let _ = writeln!(out, "report written to {}", report_path.display());
}

/// Print the detailed failure, persist the evidence, then block the run.
fn raise_blocked(
    session: &Session,
    target: &Target,
    results: &[GateResult],
    reports: &[String],
    io: &mut Io<'_>,
) -> RunError {
    let attempts = session
        .config
        .data()
        .get("failure")
        .and_then(crate::config::value::as_table)
        .and_then(|section| section.get("max_attempts_per_gate"))
        .and_then(crate::config::value::as_int);
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
    );

    let _ = writeln!(io.out, "{failure}");
    write_report(session.report_path.as_deref(), reports, io.out);
    RunError::Failure(GateFailure::new(failure, final_verdict))
}

pub fn run_session(args: &Args, runner: &dyn Runner, io: &mut Io<'_>) -> Result<i32, RunError> {
    let cwd = args
        .repo
        .clone()
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = process::workspace_root(runner, &cwd);
    let config = Config::load(&repo)?;

    for warning in config.warnings() {
        let _ = writeln!(io.err, "warning: {warning}");
    }

    let targets = detect_targets(&repo, &config);
    if args.list_targets {
        print_targets(io.out, &repo, &targets);
        return Ok(0);
    }
    if targets.is_empty() {
        return Err(RunError::Error(
            GuardrailsError::setup(format!("no cargo target found under {}", repo.display()))
                .hint("the ladder measures cargo targets; run it from the repo root"),
        ));
    }

    let session = build_session(args, &repo, config, runner);
    let Some(selected) = select_targets(args, &session, &targets, io.out)? else {
        return Ok(2);
    };

    let mut reports: Vec<String> = Vec::new();
    let mut measured = false;

    for target in &selected {
        let Some(results) = run_target(runner, &session, target, io)? else {
            continue;
        };

        measured = true;
        print_verdict(io.out, &session, target, &results);
        reports.push(markdown_report(&session, target, &results));

        if exit_code(&results) != 0 {
            return Err(raise_blocked(&session, target, &results, &reports, io));
        }
    }

    if !measured {
        let _ = writeln!(
            io.out,
            "no gate ran: no changed file belongs to any of the selected target(s).\nA gate that did not run is not a gate that passed — nothing here is ship-ready."
        );
        return Ok(2);
    }

    write_report(session.report_path.as_deref(), &reports, io.out);
    Ok(0)
}

#[cfg(test)]
mod tests;
