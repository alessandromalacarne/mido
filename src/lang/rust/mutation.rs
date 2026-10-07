//! cargo-mutants: its report files, and the arguments that scope a run.

pub mod report;

use crate::error::GuardrailsError;
use crate::lang::rust::is_source;
use crate::lang::MutationScope;
use crate::process::Command;
use crate::process::{self, Runner};
use crate::targets::{covers, Scope, Target};
use std::path::Path;

pub const TOOL: &str = "cargo-mutants";
pub const TIMEOUT_SECS: i64 = 120;

/// The arguments a declared test command carries to `cargo test`: everything
/// after the `cargo test` it names. A command that names none carries nothing —
/// the mutation tool's own default test command then applies.
pub fn test_args(command: &[String]) -> Option<Vec<String>> {
    let start = command
        .windows(2)
        .position(|pair| pair[0] == "cargo" && pair[1] == "test")?;

    Some(command[start + 2..].to_vec())
}

fn git_args(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// `cargo mutants`, scoped by the config setting: a named path, or the changed
/// files — a diff patch when there is a diff, the whole package when the run
/// reads no diff, the files themselves when the run was pointed at `--path`.
pub fn scope_args(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    scoped: MutationScope<'_>,
) -> Result<Vec<String>, GuardrailsError> {
    let MutationScope {
        configured,
        scope,
        changed,
        patch,
    } = scoped;
    if configured != "changed" {
        if configured.is_empty() || configured == "all" {
            return Ok(Vec::new());
        }
        return Ok(vec!["--file".to_string(), configured.to_string()]);
    }

    if scope == Scope::Whole {
        // No diff to patch and no path list to narrow: the whole package.
        return Ok(Vec::new());
    }

    if scope == Scope::Paths {
        let mut args = Vec::new();
        for path in changed {
            args.push("--file".to_string());
            args.push(path.clone());
        }
        return Ok(args);
    }

    changed_scope_args(runner, repo, target, patch)
}

/// The patch cargo-mutants should mutate: the working-tree diff, untracked files included.
fn changed_scope_args(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    patch: &Path,
) -> Result<Vec<String>, GuardrailsError> {
    let untracked = untracked_sources(runner, repo, target);
    stage_untracked(runner, repo, &untracked);
    write_patch(runner, repo, target, patch)?;
    release_untracked(runner, repo, &untracked);

    Ok(vec![
        "--in-diff".to_string(),
        patch.to_string_lossy().to_string(),
    ])
}

/// The untracked source files the target covers — they are part of the change.
fn untracked_sources(runner: &dyn Runner, repo: &Path, target: &Target) -> Vec<String> {
    process::git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .lines()
    .filter(|path| is_source(path) && covers(path, target))
    .map(str::to_string)
    .collect()
}

/// Intent-to-add so the diff carries the new files.
fn stage_untracked(runner: &dyn Runner, repo: &Path, untracked: &[String]) {
    if untracked.is_empty() {
        return;
    }
    let mut add = vec!["add".to_string(), "-N".to_string()];
    add.extend(untracked.iter().cloned());
    process::git(runner, repo, &git_args(&add));
}

/// Leave the index as it was found: intent-to-add entries would otherwise show
/// up as staged in whatever commit comes next.
fn release_untracked(runner: &dyn Runner, repo: &Path, untracked: &[String]) {
    if untracked.is_empty() {
        return;
    }
    let mut reset = vec!["reset".to_string(), "-q".to_string(), "--".to_string()];
    reset.extend(untracked.iter().cloned());
    process::git(runner, repo, &git_args(&reset));
}

fn write_patch(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    patch: &Path,
) -> Result<(), GuardrailsError> {
    let mut diff = vec!["git".to_string(), "diff".to_string()];
    if !target.path.is_empty() {
        // cargo-mutants resolves --in-diff paths against the cargo workspace
        // root, not the directory it runs in. `--relative` would rewrite them
        // against the target, so the patch keeps root-relative paths and the
        // target stays scoped by a pathspec.
        diff.push("--".to_string());
        diff.push(target.path.clone());
    }
    let result = runner.exec(&Command::new(repo, diff));

    std::fs::write(patch, &result.stdout).map_err(|error| {
        GuardrailsError::setup("the changed-file patch could not be written")
            .detail(format!("{}: {error}", patch.display()))
    })
}

#[cfg(test)]
mod tests;
