//! What a session prints: the target table, verdicts, and report paths.

use crate::lang::Lang;
use crate::report::{GateResult, PASS};
use crate::session::{
    default_report_path, print_targets, print_verdict, report_path, sandbox_root, scratch_dir,
    scratch_dir_in, Session,
};
use crate::style::Style;
use crate::targets::Target;
use crate::test_support::{config_for, session_for, MiniRepo};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn the_report_path_follows_the_session_scratchpad() {
    let explicit = PathBuf::from("/tmp/explicit.md");

    assert_eq!(default_report_path(Some(explicit.clone())), Some(explicit));
}

#[test]
fn targets_are_listed_with_their_kind() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let mut out = Vec::new();

    print_targets(
        &mut out,
        &repo.root,
        &Lang::Rust.detect_targets(&repo.root, &config),
        Style::plain(),
    );

    let printed = String::from_utf8(out).expect("utf8");
    assert!(printed.contains("targets under"));
    assert!(printed.contains("workspace"));
    assert!(printed.contains("member"));
}

#[test]
fn the_target_table_aligns_on_its_widest_name() {
    let targets = BTreeMap::from([
        (
            "a-very-long-target-name".to_string(),
            Target::crate_target("a-very-long-target-name", false, "Cargo.toml"),
        ),
        (
            "api".to_string(),
            Target::crate_target("api", false, "Cargo.toml"),
        ),
    ]);
    let mut out = Vec::new();

    print_targets(&mut out, Path::new("/repo"), &targets, Style::plain());

    let printed = String::from_utf8(out).expect("utf8");
    let rows: Vec<&str> = printed.lines().collect();
    assert_eq!(rows[0], "targets under /repo:");
    assert!(rows[2].starts_with("  NAME"));
    let path_column = rows[2]
        .find("PATH")
        .expect("the header names the path column");
    assert!(rows[3][path_column..].starts_with("a-very-long-target-name"));
    assert!(rows[4][path_column..].starts_with("api"));
    assert!(rows[3].ends_with("standalone crate"));
}

#[test]
fn a_report_path_is_never_relative_to_the_process_directory() {
    let repo = PathBuf::from("/repo");

    assert_eq!(
        report_path(Some(PathBuf::from("report.md")), &repo),
        Some(PathBuf::from("/repo/report.md"))
    );
    assert_eq!(
        report_path(Some(PathBuf::from("/abs/report.md")), &repo),
        Some(PathBuf::from("/abs/report.md"))
    );

    if let Some(path) = report_path(None, &repo) {
        assert!(path.is_absolute(), "{path:?} must not depend on the cwd");
    }
}

#[test]
fn a_pass_verdict_line_matches_the_gate_result() {
    let repo = MiniRepo::build(None);
    let session = Session {
        gates: Vec::new(),
        ..session_for(&repo, config_for(&repo))
    };
    let mut out = Vec::new();

    print_verdict(
        &mut out,
        &session,
        &Target::workspace_target("Cargo.toml"),
        &[GateResult::new(
            "tests",
            PASS,
            "3 passed",
            Vec::<String>::new(),
        )],
        Style::plain(),
    );

    let printed = String::from_utf8(out).expect("utf8");
    assert!(printed.starts_with("╭─ verdict"), "{printed}");
    assert!(printed.contains("│ SHIP-READY"), "{printed}");
    assert!(printed.contains("revision stamp: abc | def"), "{printed}");
}

#[test]
fn the_scratch_directory_is_a_guardrails_directory_that_exists() {
    let directory = scratch_dir();

    assert!(directory.ends_with("guardrails"));
    assert!(directory.is_dir());
    assert!(scratch_dir_in(PathBuf::from("/tmp/guardrails-test")).is_dir());
}

#[test]
fn an_empty_scratchpad_variable_is_not_a_sandbox() {
    // The environment is process-wide, so this test owns it while it runs.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var("COMMANDCODE_SCRATCHPAD").ok();

    std::env::set_var("COMMANDCODE_SCRATCHPAD", "");
    assert_eq!(sandbox_root(), None);
    assert_eq!(default_report_path(None), None);

    std::env::set_var("COMMANDCODE_SCRATCHPAD", "/tmp/sandbox");
    assert_eq!(sandbox_root(), Some(PathBuf::from("/tmp/sandbox")));
    assert_eq!(
        default_report_path(None),
        Some(PathBuf::from("/tmp/sandbox/guardrails-report.md"))
    );

    match previous {
        Some(value) => std::env::set_var("COMMANDCODE_SCRATCHPAD", value),
        None => std::env::remove_var("COMMANDCODE_SCRATCHPAD"),
    }
}
