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
fn a_test_command_carries_its_cargo_test_arguments() {
    assert_eq!(
        test_args(&cmd(&["cargo", "test", "--all-features"])),
        Some(cmd(&["--all-features"]))
    );
    assert_eq!(
        test_args(&cmd(&[
            "nix",
            "develop",
            "-c",
            "cargo",
            "test",
            "--target",
            "wasm32-unknown-unknown"
        ])),
        Some(cmd(&["--target", "wasm32-unknown-unknown"]))
    );
}

#[test]
fn a_command_that_names_no_cargo_test_carries_nothing() {
    assert_eq!(test_args(&cmd(&["cargo", "nextest", "run"])), None);
}

fn cmd(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
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
