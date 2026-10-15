//! The change a default mutation run is scoped to: the diff mido writes with
//! git and cargo-mutants reads with `--in-diff`.

use crate::error::GuardrailsError;
use crate::gates::GateRun;
use crate::lang::Lang;
use crate::process::{self, Runner};
use crate::targets::Target;
use std::path::{Path, PathBuf};

/// The change a mutation run is scoped to.
pub(super) struct ChangeScope {
    /// The diff file the passes read with `--in-diff`.
    pub(super) diff: PathBuf,
    /// Whether the diff touches a source file of the target at all.
    pub(super) has_source: bool,
}

/// The revision a change diff is taken from: the last commit. Committed work is
/// not the change — the sweep pays only for the working tree.
const CHANGE_BASE: &str = "HEAD";

/// Write the change diff cargo-mutants scopes a default run to: the working
/// tree against `HEAD`, untracked sources included.
pub(super) fn change_scope(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    lang: Lang,
) -> Result<ChangeScope, GuardrailsError> {
    let (repo, target) = (run.repo, run.target);
    let untracked = untracked_sources(runner, repo, target, lang);
    stage_untracked(runner, repo, &untracked);
    let diff = diff_text(runner, repo, target);
    release_untracked(runner, repo, &untracked);

    let path = run
        .scratch
        .join(format!("guardrails-mutants-{}.diff", target.name));
    std::fs::write(&path, &diff).map_err(|error| {
        GuardrailsError::setup("the change diff could not be written")
            .detail(format!("{}: {error}", path.display()))
            .hint(
                "the mutation gate scopes a default run to the change; without \
                 the diff it cannot decide",
            )
    })?;
    Ok(ChangeScope {
        has_source: holds_source(&diff, run, lang),
        diff: path,
    })
}

/// Whether the diff touches a source file of the target at all — the line
/// between "nothing to mutate" and "something to measure".
fn holds_source(diff: &str, run: &GateRun<'_>, lang: Lang) -> bool {
    diff.lines()
        .filter_map(|line| line.strip_prefix("+++ b/"))
        .any(|path| {
            relative_to_target(path, &run.target.path)
                .is_some_and(|relative| lang.is_source(&relative) && run.files.contains(&relative))
        })
}

/// The diff itself: root-relative paths, the target scoped by a pathspec —
/// cargo-mutants resolves `--in-diff` paths against the cargo workspace root,
/// not the directory it runs in.
fn diff_text(runner: &dyn Runner, repo: &Path, target: &Target) -> String {
    let mut args = vec!["diff".to_string(), CHANGE_BASE.to_string()];
    if !target.path.is_empty() {
        args.push("--".to_string());
        args.push(target.path.clone());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    process::git(runner, repo, &args)
}

/// The untracked source files of the target: new files are part of the change,
/// and `git diff` only shows them once the index knows them.
fn untracked_sources(runner: &dyn Runner, repo: &Path, target: &Target, lang: Lang) -> Vec<String> {
    process::git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .lines()
    .filter(|path| lang.is_source(path) && relative_to_target(path, &target.path).is_some())
    .map(str::to_string)
    .collect()
}

/// Intent-to-add so the diff carries the new files.
fn stage_untracked(runner: &dyn Runner, repo: &Path, untracked: &[String]) {
    if untracked.is_empty() {
        return;
    }
    let mut args = vec!["add", "-N"];
    args.extend(untracked.iter().map(String::as_str));
    process::git(runner, repo, &args);
}

/// Leave the index as it was found: intent-to-add entries would otherwise show
/// up as staged in whatever commit comes next.
fn release_untracked(runner: &dyn Runner, repo: &Path, untracked: &[String]) {
    if untracked.is_empty() {
        return;
    }
    let mut args = vec!["reset", "-q", "--"];
    args.extend(untracked.iter().map(String::as_str));
    process::git(runner, repo, &args);
}

/// A repo-root path as the target's own file list spells it, when it is the
/// target's at all.
fn relative_to_target(path: &str, target_dir: &str) -> Option<String> {
    if target_dir.is_empty() {
        return Some(path.to_string());
    }
    path.strip_prefix(target_dir)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_string)
}
