//! Git queries: the revision stamp and the files git sees.

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

/// The refs a default branch can go by, best first: the remote's own HEAD,
/// then the names it usually points at.
const DEFAULT_BRANCHES: [&str; 5] = [
    "origin/HEAD",
    "origin/main",
    "origin/master",
    "main",
    "master",
];

/// The revision a change diff is taken from: the merge base with the repo's
/// default branch when one resolves, `HEAD` when none does — so committed
/// branch work and uncommitted work both count as the change.
pub fn change_base(runner: &dyn Runner, repo: &Path) -> String {
    for candidate in DEFAULT_BRANCHES {
        if git(
            runner,
            repo,
            &["rev-parse", "--verify", "--quiet", candidate],
        )
        .trim()
        .is_empty()
        {
            continue;
        }
        let base = git(runner, repo, &["merge-base", "HEAD", candidate]);
        if !base.trim().is_empty() {
            return base.trim().to_string();
        }
    }
    "HEAD".to_string()
}
