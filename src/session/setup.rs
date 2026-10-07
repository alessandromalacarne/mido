//! Resolving a run: its scope, its revision, and the targets it measures.

use super::{report_path, scratch_dir, Session};
use crate::cli::Args;
use crate::config::Config;
use crate::error::GuardrailsError;
use crate::gate::{Gate, GATES};
use crate::lang::Lang;
use crate::process::{self, Runner};
use crate::report::MAX_BANNER_FILES;
use crate::style::Style;
use crate::targets::{explicit_paths, pick_auto_target, Scope, Target};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub fn build_session(
    args: &Args,
    repo: &Path,
    config: Config,
    runner: &dyn Runner,
    lang: Lang,
) -> Result<Session, GuardrailsError> {
    let (scope, base, changed) = resolve_scope(args, repo, &config, runner, lang)?;

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
        gates: selected_gates(args),
        scratch: scratch_dir(),
        baseline_lcov: args.baseline_lcov.clone(),
        report_path: report_path(args.report.clone(), repo),
        apply_aid: args.apply_workspace_aid,
        as_json: args.json,
        selection: selection(args),
    })
}

/// The files this run measures: the diff against a base, the `--path` list — a
/// path list is the whole scope, so no base is picked and no diff is read — or,
/// with `--no-diff`, every file git sees.
fn resolve_scope(
    args: &Args,
    repo: &Path,
    config: &Config,
    runner: &dyn Runner,
    lang: Lang,
) -> Result<(Scope, String, Vec<String>), GuardrailsError> {
    if !args.path.is_empty() {
        let changed = explicit_paths(repo, config, &args.path, lang)?;
        return Ok((Scope::Paths, String::new(), changed));
    }

    if args.no_diff {
        let changed = process::all_files(runner, repo);
        return Ok((Scope::Whole, String::new(), changed));
    }

    let base = args
        .base
        .clone()
        .unwrap_or_else(|| process::pick_base(runner, repo));
    let changed = process::changed_files(runner, repo, &base);
    Ok((Scope::Diff, base, changed))
}

/// The gates to run: what `--gate` selected, or the whole ladder.
fn selected_gates(args: &Args) -> Vec<Gate> {
    if args.gates.is_empty() {
        GATES.to_vec()
    } else {
        args.gates.clone()
    }
}

/// How the target was chosen, as the banner spells it.
fn selection(args: &Args) -> String {
    if args.target == "auto" {
        "auto"
    } else {
        "requested"
    }
    .to_string()
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
        return Ok(Some(all_targets(targets)));
    }

    if session.scope == Scope::Whole {
        return Ok(Some(vec![workspace_roll_up(targets)?]));
    }

    match pick_auto_target(targets, &session.changed)? {
        Some(inferred) => Ok(Some(vec![inferred])),
        None => {
            report_nothing_to_measure(out, session, style);
            Ok(None)
        }
    }
}

/// Every target `--all` measures: the crates, not the workspace roll-up.
fn all_targets(targets: &BTreeMap<String, Target>) -> Vec<Target> {
    targets
        .values()
        .filter(|target| !target.path.is_empty() || target.workspace_member)
        .cloned()
        .collect()
}

/// The auto target of a whole-target run: with no diff to infer from, the
/// workspace roll-up is the repo's own target.
fn workspace_roll_up(targets: &BTreeMap<String, Target>) -> Result<Target, GuardrailsError> {
    targets
        .values()
        .find(|target| target.path.is_empty())
        .cloned()
        .ok_or_else(|| {
            GuardrailsError::setup("a whole-target run has no workspace target to fall back on")
                .hint("name a target explicitly, or run with --all")
        })
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

    if session.scope == Scope::Whole {
        let _ = writeln!(
            out,
            "nothing to measure: git lists no file in this repository — no tracked file, no untracked one.\n"
        );
        let _ = writeln!(
            out,
            "{}",
            style.dim(
                "The whole-target run measures the files git sees; --path names files directly."
            )
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
