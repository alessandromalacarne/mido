//! cargo-mutants: its summary line, and the arguments that scope it.

pub mod report;

use crate::error::GuardrailsError;
use crate::lang::rust::is_source;
use crate::lang::{MutationScope, MutationSummary};
use crate::process::Command;
use crate::process::{self, Runner};
use crate::targets::{covers, Scope, Target};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

pub const TOOL: &str = "cargo-mutants";
pub const TIMEOUT_SECS: i64 = 120;

/// The summary line: `115 mutants tested in 6m: 96 caught, 19 unviable` —
/// cargo-mutants leaves zero-valued categories (`0 missed`) out.
fn summary_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"(\d+) mutants tested in [^:]+: (.+)").expect("valid pattern"))
}

fn count_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(\d+) (caught|missed|unviable)").expect("valid pattern"))
}

/// The line an `--iterate` run prints for the mutants it did not rerun.
fn iteration_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"Iteration excludes (\d+) previously caught or unviable mutants")
            .expect("valid pattern")
    })
}

/// The counts a cargo-mutants run reported, when it printed a summary.
pub fn mutation_summary(output: &str) -> Option<MutationSummary> {
    let summary = summary_pattern().captures(output)?;
    let total: i64 = summary[1].parse().unwrap_or_default();
    let mut caught = 0;
    let mut missed = 0;
    let mut unviable = 0;
    for count in count_pattern().captures_iter(&summary[2]) {
        let value: i64 = count[1].parse().unwrap_or_default();
        match &count[2] {
            "caught" => caught = value,
            "missed" => missed = value,
            _ => unviable = value,
        }
    }
    let skipped = iteration_pattern()
        .captures(output)
        .map(|captures| captures[1].parse().unwrap_or_default())
        .unwrap_or_default();
    Some(MutationSummary {
        total,
        caught,
        missed,
        unviable,
        skipped,
    })
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
mod tests {
    use super::*;
    use crate::process::Outcome;
    use crate::test_support::{FakeRunner, MiniRepo};
    use std::path::PathBuf;

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    fn scratch(repo: &MiniRepo) -> PathBuf {
        let scratch = repo.root.join("scratch");
        std::fs::create_dir_all(&scratch).expect("scratch");
        scratch
    }

    #[test]
    fn a_summary_that_omits_the_zero_categories_is_read() {
        let summary = mutation_summary("115 mutants tested in 6m: 96 caught, 19 unviable\n")
            .expect("summary");

        assert_eq!(summary.total, 115);
        assert_eq!(summary.caught, 96);
        assert_eq!(summary.missed, 0);
        assert_eq!(summary.unviable, 19);
    }

    #[test]
    fn output_without_a_summary_line_is_nothing() {
        assert_eq!(mutation_summary("cargo-mutants: nothing to do"), None);
    }

    #[test]
    fn a_whole_target_scope_mutates_the_package_without_a_patch() {
        let repo = repo();
        let runner = FakeRunner::with(&[("git diff", 0, "diff --git a/src/foo.rs b/src/foo.rs\n")]);

        let args = scope_args(
            &runner,
            &repo.root,
            &Target::workspace_target(crate::lang::rust::MANIFEST),
            MutationScope {
                configured: "changed",
                scope: Scope::Whole,
                changed: &["src/foo.rs".to_string()],
                patch: &scratch(&repo).join("patch"),
            },
        )
        .expect("no patch to write");

        assert!(args.is_empty(), "{args:?}");
        assert!(!runner.called_with("git diff"), "no diff is read");
    }

    #[test]
    fn the_changed_scope_writes_the_working_tree_patch_first() {
        let repo = repo();
        let runner = FakeRunner::with(&[("git diff", 0, "diff --git a/src/foo.rs b/src/foo.rs\n")]);
        let scratch = scratch(&repo);

        let args = scope_args(
            &runner,
            &repo.root,
            &Target::workspace_target(crate::lang::rust::MANIFEST),
            MutationScope {
                configured: "changed",
                scope: Scope::Diff,
                changed: &[],
                patch: &scratch.join("patch"),
            },
        )
        .expect("patch written");

        assert!(args[0] == "--in-diff" && args[1].ends_with("patch"));
        let patch = std::fs::read_to_string(scratch.join("patch")).expect("patch");
        assert!(patch.contains("diff --git"));
    }

    #[test]
    fn the_member_patch_is_filtered_without_rewriting_the_paths() {
        let repo = repo();
        let runner = FakeRunner::with(&[(
            "git diff",
            0,
            "diff --git a/frontend/src/foo.rs b/frontend/src/foo.rs\n",
        )]);
        let scratch = scratch(&repo);

        scope_args(
            &runner,
            &repo.root,
            &Target::crate_target("frontend", true, crate::lang::rust::MANIFEST),
            MutationScope {
                configured: "changed",
                scope: Scope::Diff,
                changed: &[],
                patch: &scratch.join("patch"),
            },
        )
        .expect("patch written");

        assert!(
            runner.called_with("git diff -- frontend"),
            "the pathspec scopes the patch without rewriting its paths"
        );
        assert!(
            !runner.called_with("--relative"),
            "the patch must stay relative to the cargo workspace root: that is \
             the root cargo-mutants resolves --in-diff paths against"
        );
    }

    #[test]
    fn untracked_rust_files_are_added_to_the_index_and_then_released() {
        let repo = repo();
        let runner = FakeRunner {
            responses: vec![
                (
                    "ls-files".to_string(),
                    Outcome::new(0, "frontend/src/new.rs\nfrontend/NOTES.md\nREADME.md\n", ""),
                ),
                ("git diff".to_string(), Outcome::new(0, "diff\n", "")),
            ],
            ..FakeRunner::default()
        };

        scope_args(
            &runner,
            &repo.root,
            &Target::crate_target("frontend", false, crate::lang::rust::MANIFEST),
            MutationScope {
                configured: "changed",
                scope: Scope::Diff,
                changed: &[],
                patch: &scratch(&repo).join("patch"),
            },
        )
        .expect("patch written");

        assert!(runner.called_with("add -N frontend/src/new.rs"));
        assert!(
            !runner.called_with("frontend/NOTES.md"),
            "only source files are staged, even when the target covers them"
        );
        assert!(!runner.called_with("add -N README.md"));
        assert!(runner.called_with("reset -q -- frontend/src/new.rs"));
        assert!(runner.called_with("git diff -- frontend"));
    }
}
