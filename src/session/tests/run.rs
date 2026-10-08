//! The session as the CLI drives it: argv in, exit code and output out.

use crate::test_support::{repo_runner, run_cli as run, FakeRunner, MiniRepo};

/// A tests-gate config, so fixture runs are cheap and their output is stable.
fn tests_config() -> Option<&'static str> {
    Some(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
    )
}

#[test]
fn list_targets_prints_the_detected_set() {
    let repo = MiniRepo::build(None);
    let (code, out, _) = run(
        &["--repo", &repo.root.to_string_lossy(), "--list-targets"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 0);
    assert!(out.contains("frontend"));
    assert!(out.contains("workspace"));
    assert!(out.contains("standalone crate"));
}

#[test]
fn a_config_without_a_version_warns_on_stderr() {
    let repo = MiniRepo::build(Some(
        "
            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "--list-targets"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 0);
    assert!(err.contains("warning:"), "{err}");
    assert!(err.contains("version"), "{err}");
}

#[test]
fn an_unknown_package_exits_2_with_a_detailed_error() {
    let repo = MiniRepo::build(None);
    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "-p", "nope"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 2);
    assert!(err.contains("package `nope` not found"), "{err}");
    assert!(err.contains("frontend"), "{err}");
}

#[test]
fn invalid_config_exits_2_with_a_detailed_error() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [analysis]
            min_mi = 20
        ",
    ));
    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "-p", "frontend"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 2);
    assert!(err.contains("min_mi"));
    assert!(err.contains("unknown key `min_mi`"));
    assert!(err.contains("line 5: unknown key `min_mi`"));
}

#[test]
fn a_run_with_no_visible_file_measures_nothing() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("hash-object", 0, "dirtyhash\n")]);

    let (code, out, _) = run(
        &["--repo", &repo.root.to_string_lossy(), "--gate", "tests"],
        &runner,
    );

    assert_eq!(code, 2, "{out}");
    assert!(out.contains("git lists no file"), "{out}");
    assert!(out.contains("nothing to measure"), "{out}");
}

#[test]
fn the_workspace_measures_every_file_it_owns() {
    let repo = MiniRepo::build(tests_config());
    let runner = repo_runner(
        "lib/src/foo.rs\nlib/Cargo.toml\nfrontend/src/main.rs\n",
        (0, "test result: ok. 3 passed; 0 failed\n"),
    );

    let (code, out, _) = run(
        &["--repo", &repo.root.to_string_lossy(), "--gate", "tests"],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("workspace (./)"), "{out}");
    assert!(out.contains("measured 2 files (1 rust)"), "{out}");
    assert!(out.contains("  lib/src/foo.rs"), "{out}");
    assert!(
        !out.contains("frontend"),
        "a standalone crate's files belong to it, not to the workspace: {out}"
    );
    assert!(
        runner.called_with("ls-files"),
        "the file set comes from git"
    );
}

#[test]
fn a_package_measures_its_whole_directory() {
    let repo = MiniRepo::build(tests_config());
    let runner = repo_runner(
        "lib/src/foo.rs\nlib/src/bar.rs\nfrontend/src/main.rs\n",
        (0, "test result: ok. 3 passed; 0 failed\n"),
    );

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "-p",
            "lib",
            "--gate",
            "tests",
        ],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("lib (lib/)"), "{out}");
    assert!(out.contains("measured 2 files (2 rust)"), "{out}");
    assert!(out.contains("  src/foo.rs"), "{out}");
    assert!(out.contains("  src/bar.rs"), "{out}");
    assert!(!out.contains("main.rs"), "{out}");
}

#[test]
fn several_packages_are_measured_in_turn() {
    let repo = MiniRepo::build(tests_config());
    let runner = repo_runner(
        "lib/src/foo.rs\napi/src/bar.rs\n",
        (0, "test result: ok. 1 passed; 0 failed\n"),
    );

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "-p",
            "lib",
            "-p",
            "api",
            "--gate",
            "tests",
        ],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("lib (lib/)"), "{out}");
    assert!(out.contains("api (api/)"), "{out}");
    assert_eq!(out.matches("SHIP-READY").count(), 2, "{out}");
}

#[test]
fn a_passing_run_writes_the_report_and_says_so() {
    let repo = MiniRepo::build(tests_config());
    let report = repo.root.join("report.md");
    let runner = repo_runner(
        "lib/src/foo.rs\n",
        (0, "test result: ok. 3 passed; 0 failed\n"),
    );

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "--gate",
            "tests",
            "--report",
            &report.to_string_lossy(),
            "--json",
        ],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("│ SHIP-READY"), "{out}");
    assert!(out.contains("\"verdict\":\"SHIP-READY\""));
    assert!(out.contains("report written to"));
    let markdown = std::fs::read_to_string(&report).expect("report");
    assert!(markdown.contains("VERDICT: SHIP-READY"));
    assert!(
        markdown.contains("measured files: 1 (1 rust)"),
        "{markdown}"
    );
}

#[test]
fn a_failing_gate_exits_1_with_the_failure_report_on_stdout() {
    let repo = MiniRepo::build(tests_config());
    let runner = repo_runner(
        "lib/src/foo.rs\n",
        (101, "test result: FAILED. 0 passed; 2 failed\n"),
    );
    let report = repo.root.join("report.md");

    let (code, out, err) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "--gate",
            "tests",
            "--report",
            &report.to_string_lossy(),
        ],
        &runner,
    );

    assert_eq!(code, 1);
    assert!(out.contains("╭─ failure"), "{out}");
    assert!(out.contains("[4/6] ✗ tests — FAIL"), "{out}");
    assert!(err.contains("error: guardrails BLOCKED"));
    assert!(std::fs::read_to_string(&report)
        .expect("report")
        .contains("VERDICT: BLOCKED — tests=FAIL"));
}

#[test]
fn a_gate_that_never_reached_a_verdict_exits_2_instead_of_1() {
    let repo = MiniRepo::build(None);
    std::fs::create_dir_all(repo.root.join("lib/src")).expect("member src dir");
    std::fs::write(
        repo.root.join("lib/src/foo.rs"),
        "pub fn foo() -> i64 {\n    1\n}\n",
    )
    .expect("measured file");
    let runner = FakeRunner::with(&[
        ("ls-files", 0, "lib/src/foo.rs\n"),
        ("hash-object", 0, "dirtyhash\n"),
    ]);

    let (code, out, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "--gate", "size"],
        &runner,
    );

    assert_eq!(code, 2, "INCOMPLETE is not a FAIL: {out}");
    assert!(out.contains("BLOCKED — size=INCOMPLETE"), "{out}");
    assert!(err.contains("error: guardrails BLOCKED"), "{err}");
}

#[test]
fn a_declared_target_without_a_manifest_is_a_setup_error() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [targets.tui]
            path = \"cli\"
        ",
    ));
    std::fs::remove_file(repo.root.join("cli/Cargo.toml")).expect("manifest removed");
    let runner = repo_runner(
        "lib/src/foo.rs\n",
        (0, "test result: ok. 1 passed; 0 failed\n"),
    );

    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "-p", "tui"],
        &runner,
    );

    assert_eq!(code, 2);
    assert!(err.contains("Cargo.toml"), "{err}");
}
