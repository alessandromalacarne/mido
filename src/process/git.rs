//! Git queries: the revision stamp, the changed files, the base ref.

use super::{Command, Runner};
use std::path::{Path, PathBuf};

pub fn git(runner: &dyn Runner, repo: &Path, args: &[&str]) -> String {
    let command_args = std::iter::once("git").chain(args.iter().copied());
    let result = runner.exec(&Command::new(repo, command_args));
    if result.ok() {
        result.stdout
    } else {
        String::new()
    }
}

/// The revision stamp: `git status --short | git hash-object --stdin`.
pub fn dirty_hash(runner: &dyn Runner, repo: &Path) -> String {
    let status = runner
        .exec(&Command::new(repo, ["git", "status", "--short"]))
        .stdout;
    runner
        .exec(&Command::new(repo, ["git", "hash-object", "--stdin"]).stdin(status))
        .stdout
        .trim()
        .to_string()
}

pub fn workspace_root(runner: &dyn Runner, cwd: &Path) -> PathBuf {
    let located = git(runner, cwd, &["rev-parse", "--show-toplevel"]);
    let trimmed = located.trim();
    if trimmed.is_empty() {
        cwd.to_path_buf()
    } else {
        PathBuf::from(trimmed)
    }
}

/// Repo-relative files changed against `base`, working tree included.
pub fn changed_files(runner: &dyn Runner, repo: &Path, base: &str) -> Vec<String> {
    let merge_base = git(runner, repo, &["merge-base", "HEAD", base]);
    let merge_base = {
        let trimmed = merge_base.trim();
        if trimmed.is_empty() {
            "HEAD".to_string()
        } else {
            trimmed.to_string()
        }
    };

    let committed = git(runner, repo, &["diff", "--name-only", &merge_base]);
    let staged = git(runner, repo, &["diff", "--name-only", "--cached"]);
    let untracked = git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    );

    let mut files: Vec<String> = [committed, staged, untracked]
        .iter()
        .flat_map(|output| output.lines().map(|line| line.to_string()))
        .filter(|line| !line.is_empty())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Every file git can see — the tracked ones, plus untracked ones that are not
/// ignored.
pub fn all_files(runner: &dyn Runner, repo: &Path) -> Vec<String> {
    let tracked = git(runner, repo, &["ls-files"]);
    let untracked = git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    );

    let mut files: Vec<String> = [tracked, untracked]
        .iter()
        .flat_map(|output| output.lines().map(|line| line.to_string()))
        .filter(|line| !line.is_empty())
        .collect();
    files.sort();
    files.dedup();
    files
}

pub fn pick_base(runner: &dyn Runner, repo: &Path) -> String {
    for candidate in ["origin/mvp", "origin/develop", "origin/master"] {
        let result = runner.exec(&Command::new(
            repo,
            ["git", "rev-parse", "--verify", "--quiet", candidate],
        ));
        if result.ok() {
            return candidate.to_string();
        }
    }
    "HEAD".to_string()
}
