//! The session as the CLI drives it: argv in, exit code and output out.

use crate::test_support::{changed_runner, run_cli as run, FakeRunner, MiniRepo};

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
fn unknown_target_exits_2_with_a_detailed_error() {
    let repo = MiniRepo::build(None);
    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "nope"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 2);
    assert!(err.contains("error:"));
    assert!(err.contains("frontend"));
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
        &["--repo", &repo.root.to_string_lossy(), "frontend"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 2);
    assert!(err.contains("min_mi"));
    assert!(err.contains("unknown key `min_mi`"));
    assert!(err.contains("line 5: unknown key `min_mi`"));
}

#[test]
fn no_changed_files_exits_2_instead_of_claiming_a_pass() {
    let repo = MiniRepo::build(None);
    let _ = repo.git();
    let (code, out, _) = run(
        &["--repo", &repo.root.to_string_lossy(), "workspace"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 2);
    assert!(out.to_lowercase().contains("nothing"));
}

#[test]
fn a_target_owning_none_of_the_diff_leaves_nothing_ship_ready() {
    let repo = MiniRepo::build(None);
    let runner = changed_runner((0, "test result: ok. 1 passed; 0 failed\n"));

    let (code, out, _) = run(
        &["--repo", &repo.root.to_string_lossy(), "frontend"],
        &runner,
    );

    assert_eq!(code, 2);
    assert!(out.contains("no gate ran"), "{out}");
    assert!(out.contains("nothing here is ship-ready"), "{out}");
}

#[test]
fn a_no_diff_run_measures_the_whole_target() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    let runner = FakeRunner::with(&[
        ("--others", 0, ""),
        ("ls-files", 0, "lib/src/foo.rs\nlib/src/bar.rs\n"),
        ("hash-object", 0, "dirtyhash\n"),
        ("cargo test", 0, "test result: ok. 3 passed; 0 failed\n"),
    ]);

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "lib",
            "--no-diff",
            "--gate",
            "tests",
        ],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("whole target (no diff)"), "{out}");
    assert!(out.contains("src/foo.rs"), "{out}");
    assert!(out.contains("src/bar.rs"), "{out}");
    assert!(
        runner.called_with("ls-files"),
        "the file set comes from git"
    );
    assert!(!runner.called_with("merge-base"), "no diff was read");
}

#[test]
fn a_no_diff_run_can_measure_one_file() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    std::fs::create_dir_all(repo.root.join("lib/src")).expect("member src dir");
    std::fs::write(
        repo.root.join("lib/src/bar.rs"),
        "pub fn bar() -> i64 {\n    1\n}\n",
    )
    .expect("named file");
    let runner = FakeRunner::with(&[
        ("hash-object", 0, "dirtyhash\n"),
        ("cargo test", 0, "test result: ok. 1 passed; 0 failed\n"),
    ]);

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "--no-diff",
            "--path",
            "lib/src/bar.rs",
            "--gate",
            "tests",
        ],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("explicit paths"), "{out}");
    assert!(out.contains("src/bar.rs"), "{out}");
    assert!(!out.contains("src/foo.rs"), "{out}");
}

#[test]
fn a_no_diff_run_defaults_to_the_workspace_roll_up() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    let runner = FakeRunner::with(&[
        ("--others", 0, ""),
        ("ls-files", 0, "lib/src/foo.rs\n"),
        ("hash-object", 0, "dirtyhash\n"),
        ("cargo test", 0, "test result: ok. 1 passed; 0 failed\n"),
    ]);

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "--no-diff",
            "--gate",
            "tests",
        ],
        &runner,
    );

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("whole target (no diff)"), "{out}");
    assert!(out.contains("workspace (./)"), "{out}");
}

#[test]
fn a_no_diff_run_with_no_visible_file_measures_nothing() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("hash-object", 0, "dirtyhash\n")]);

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "--no-diff",
            "--gate",
            "tests",
        ],
        &runner,
    );

    assert_eq!(code, 2, "{out}");
    assert!(out.contains("git lists no file"), "{out}");
    assert!(out.contains("nothing to measure"), "{out}");
}

#[test]
fn a_passing_run_writes_the_report_and_says_so() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    let report = repo.root.join("report.md");
    let runner = changed_runner((0, "test result: ok. 3 passed; 0 failed\n"));

    let (code, out, _) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "workspace",
            "--gate",
            "tests",
            "--report",
            &report.to_string_lossy(),
            "--json",
        ],
        &runner,
    );

    assert_eq!(code, 0);
    assert!(out.contains("│ SHIP-READY"), "{out}");
    assert!(out.contains("\"verdict\":\"SHIP-READY\""));
    assert!(out.contains("report written to"));
    let markdown = std::fs::read_to_string(&report).expect("report");
    assert!(markdown.contains("VERDICT: SHIP-READY"));
}

#[test]
fn a_failing_gate_exits_1_with_the_failure_report_on_stdout() {
    let repo = MiniRepo::build(Some(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    let runner = changed_runner((101, "test result: FAILED. 0 passed; 2 failed\n"));
    let report = repo.root.join("report.md");

    let (code, out, err) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "workspace",
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
    .expect("changed file");
    let runner = changed_runner((0, ""));

    let (code, out, err) = run(
        &[
            "--repo",
            &repo.root.to_string_lossy(),
            "workspace",
            "--gate",
            "size",
        ],
        &runner,
    );

    assert_eq!(code, 2, "INCOMPLETE is not a FAIL: {out}");
    assert!(out.contains("BLOCKED — size=INCOMPLETE"), "{out}");
    assert!(err.contains("error: guardrails BLOCKED"), "{err}");
}

#[test]
fn a_target_without_a_manifest_is_a_setup_error() {
    let repo = MiniRepo::build(None);
    std::fs::remove_file(repo.root.join("frontend/Cargo.toml")).expect("manifest removed");
    let runner = changed_runner((0, "test result: ok. 1 passed; 0 failed\n"));

    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "frontend"],
        &runner,
    );

    assert_eq!(code, 2);
    assert!(err.contains("Cargo.toml"));
}
