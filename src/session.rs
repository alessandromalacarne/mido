//! One run of the ladder: what it measures, in what order, and what it prints.

use crate::cli::Args;
use crate::config::Config;
use crate::error::{GateFailure, GuardrailsError, RunError};
use crate::gates::{run_gates, GateRun};
use crate::lang::Lang;
use crate::process::{self, Runner};
use crate::report::{
    exit_code, render_banner, render_failure, render_report_markdown, render_verdict, verdict,
    BannerContext, FailureContext, GateResult, ReportContext, GATES, MAX_BANNER_FILES,
};
use crate::style::Style;
use crate::targets::{explicit_paths, pick_auto_target, scope_changed, Scope, Target};
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

pub fn build_session(
    args: &Args,
    repo: &Path,
    config: Config,
    runner: &dyn Runner,
    lang: Lang,
) -> Result<Session, GuardrailsError> {
    // `--path` is the whole scope: no base is picked and no diff is read.
    let (scope, base, changed) = if args.path.is_empty() {
        let base = args
            .base
            .clone()
            .unwrap_or_else(|| process::pick_base(runner, repo));
        let changed = process::changed_files(runner, repo, &base);
        (Scope::Diff, base, changed)
    } else {
        (
            Scope::Paths,
            String::new(),
            explicit_paths(repo, &config, &args.path, lang)?,
        )
    };

    Ok(Session {
        repo: repo.to_path_buf(),
        config,
        lang,
        scope,
        base,
        changed,
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
    })
}

pub fn print_targets(
    out: &mut dyn Write,
    repo: &Path,
    targets: &BTreeMap<String, Target>,
    style: Style,
) {
    let rows: Vec<(String, String, &'static str)> = targets
        .iter()
        .map(|(name, target)| {
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
            (name.clone(), path.to_string(), kind)
        })
        .collect();
    let name_width = rows.iter().map(|row| row.0.len()).max().unwrap_or(0).max(4);
    let path_width = rows.iter().map(|row| row.1.len()).max().unwrap_or(0).max(4);

    let _ = writeln!(out, "targets under {}:", repo.display());
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        style.dim(&format!(
            "  {:<name_width$} {:<path_width$} KIND",
            "NAME", "PATH"
        ))
    );
    for (name, path, kind) in rows {
        let _ = writeln!(out, "  {name:<name_width$} {path:<path_width$} {kind}");
    }
}

/// Which targets this run measures; `None` means "nothing to measure here".
pub fn select_targets(
    args: &Args,
    session: &Session,
    targets: &BTreeMap<String, Target>,
    out: &mut dyn Write,
    style: Style,
) -> Result<Option<Vec<Target>>, GuardrailsError> {
    // Opening a named target is checked first: a typo is a typo whether or not
    // the tree happens to have changes.
    let requested = requested_target(args, session)?;

    if session.changed.is_empty() {
        report_nothing_changed(out, session, style);
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
            report_nothing_to_measure(out, session, style);
            Ok(None)
        }
    }
}

/// The target named on the command line, if one was named.
fn requested_target(args: &Args, session: &Session) -> Result<Option<Target>, GuardrailsError> {
    if args.all || args.target == "auto" {
        return Ok(None);
    }
    Ok(Some(session.lang.resolve_target(
        &session.repo,
        &session.config,
        &args.target,
    )?))
}

/// Nobody can measure a revision that is not there.
fn report_nothing_changed(out: &mut dyn Write, session: &Session, style: Style) {
    if session.scope == Scope::Paths {
        let _ = writeln!(out, "nothing to measure: the paths given hold no file.\n");
        let _ = writeln!(
            out,
            "{}",
            style.dim("Pass a file, a folder with files in it, or a target name (--list-targets).")
        );
        return;
    }

    let _ = writeln!(
        out,
        "nothing changed against {} — no revision to measure.",
        session.base
    );
    let _ = writeln!(
        out,
        "{}",
        style.dim("Pass --base <sha before the change>, or check out a branch with the work.")
    );
}

/// No changed file belongs to a cargo target — with the list to prove it.
fn report_nothing_to_measure(out: &mut dyn Write, session: &Session, style: Style) {
    let listing: Vec<String> = session
        .changed
        .iter()
        .take(MAX_BANNER_FILES)
        .map(|path| style.dim(&format!("  {path}")))
        .collect();
    let _ = writeln!(
        out,
        "nothing to measure: no changed file belongs to a cargo target.\n{}\nName a target explicitly (--list-targets) if the ladder should run anyway.",
        listing.join("\n")
    );
}

pub fn markdown_report(session: &Session, target: &Target, results: &[GateResult]) -> String {
    let changed = scope_changed(&session.changed, target);
    render_report_markdown(
        results,
        &ReportContext {
            target,
            base: &session.base,
            revision: &session.revision,
            dirty: &session.dirty,
            changed: &changed,
            source_label: session.lang.source_label(),
            source_count: changed
                .iter()
                .filter(|path| session.lang.is_source(path))
                .count(),
        },
    )
}

pub fn print_verdict(
    out: &mut dyn Write,
    session: &Session,
    target: &Target,
    results: &[GateResult],
    style: Style,
) {
    let final_verdict = verdict(results);
    let _ = writeln!(out, "{}", render_verdict(&final_verdict, style));
    let _ = writeln!(
        out,
        "{}",
        style.dim(&format!(
            "revision stamp: {} | {}",
            session.revision, session.dirty
        ))
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
        target_banner(session, target, &scoped, io.style)
    );
    let _ = writeln!(io.out);

    if scoped.is_empty() {
        let _ = writeln!(io.out, "{}\n", skip_note(target, io.style));
        return Ok(None);
    }

    session
        .lang
        .validate_target_setup(runner, &session.repo, target, session.apply_aid, io.out)?;

    let results = run_gates(
        runner,
        &GateRun {
            repo: &session.repo,
            target,
            config: &session.config,
            lang: session.lang,
            scope: session.scope,
            changed: &scoped,
            gates: &session.gates,
            scratch: &session.scratch,
            baseline_lcov: session.baseline_lcov.as_deref(),
        },
        io.out,
        io.style,
    );
    Ok(Some(results))
}

/// The banner a target's run opens with.
fn target_banner(session: &Session, target: &Target, scoped: &[String], style: Style) -> String {
    let source_count = scoped
        .iter()
        .filter(|path| session.lang.is_source(path))
        .count();
    render_banner(
        target,
        &BannerContext {
            base: &session.base,
            revision: &session.revision,
            dirty: &session.dirty,
            changed: scoped,
            selected_how: &session.selection,
            source_label: session.lang.source_label(),
            source_count,
        },
        style,
    )
}

/// A target that owns none of the diff is skipped with a note, never silently.
fn skip_note(target: &Target, style: Style) -> String {
    style.dim(&format!(
        "no changed file belongs to {} — skipping.",
        target.label()
    ))
}

pub fn write_report(
    report_path: Option<&Path>,
    reports: &[String],
    out: &mut dyn Write,
    style: Style,
) {
    let Some(report_path) = report_path else {
        return;
    };
    if let Some(parent) = report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(report_path, reports.join("\n"));
    let _ = writeln!(
        out,
        "{}",
        style.dim(&format!("report written to {}", report_path.display()))
    );
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

pub fn run_session(args: &Args, runner: &dyn Runner, io: &mut Io<'_>) -> Result<i32, RunError> {
    let cwd = args
        .repo
        .clone()
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = process::workspace_root(runner, &cwd);
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
    let Some(selected) = select_targets(args, &session, &targets, io.out, io.style)? else {
        return Ok(2);
    };

    let (measured, reports) = measure(runner, &session, &selected, io)?;
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
