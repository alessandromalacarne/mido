use super::*;
use crate::cli::parse_from;
use crate::report::PASS;
use crate::targets::detect_targets;
use crate::test_support::{FakeRunner, MiniRepo};

fn drains() -> (Vec<u8>, Vec<u8>) {
    (Vec::new(), Vec::new())
}

fn run(argv: &[&str], runner: &FakeRunner) -> (i32, String, String) {
    let args = parse_from(argv);
    let (mut out, mut err) = drains();
    let code = crate::cli::main_with(&args, runner, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).to_string(),
        String::from_utf8_lossy(&err).to_string(),
    )
}

/// A runner whose git answers say "one changed file under lib/".
fn changed_runner(tests: (i32, &str)) -> FakeRunner {
    FakeRunner::with(&[
        ("merge-base", 0, "base\n"),
        ("--name-only", 0, "lib/src/foo.rs\n"),
        ("hash-object", 0, "dirtyhash\n"),
        ("cargo test", tests.0, tests.1),
    ])
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
fn a_gate_name_that_does_not_exist_never_reaches_the_ladder() {
    let rejected = crate::cli::try_parse_from(&["--gate", "lint"]);

    assert!(rejected.is_err(), "an unknown gate is not a gate");
    assert_eq!(rejected.expect_err("rejected").exit_code(), 2);
}

#[test]
fn gate_selection_narrows_the_ladder() {
    let args = parse_from(&["--gate", "size", "--gate", "syntax"]);

    assert_eq!(
        args.gates
            .iter()
            .map(|gate| gate.name())
            .collect::<Vec<_>>(),
        vec!["size", "syntax"]
    );
}

#[test]
fn a_run_without_a_gate_selection_uses_all_six() {
    let repo = MiniRepo::build(None);
    let session = build_session(
        &parse_from(&["--repo", "."]),
        &repo.root,
        Config::load(&repo.root).expect("config"),
        &FakeRunner::default(),
    );

    assert_eq!(session.gates.len(), GATES.len());
}

#[test]
fn a_target_owning_none_of_the_diff_is_skipped_with_a_note() {
    let repo = MiniRepo::build(None);
    let config = Config::load(&repo.root).expect("config loads");
    let session = Session {
        repo: repo.root.clone(),
        config,
        base: "HEAD".to_string(),
        changed: vec!["lib/src/foo.rs".to_string()],
        revision: "abc".to_string(),
        dirty: "def".to_string(),
        gates: GATES.iter().map(|gate| (*gate).to_string()).collect(),
        scratch: repo.root.join("scratch"),
        baseline_lcov: None,
        report_path: None,
        apply_aid: false,
        as_json: false,
        selection: "auto".to_string(),
    };
    let (mut out, mut err) = drains();
    let mut io = Io {
        out: &mut out,
        err: &mut err,
    };

    let results = run_target(
        &FakeRunner::default(),
        &session,
        &Target::crate_target("frontend", false),
        &mut io,
    )
    .expect("no setup error");

    assert!(results.is_none());
    assert!(
        String::from_utf8_lossy(&out).contains("no changed file belongs to frontend (frontend/)")
    );
}

#[test]
fn the_report_path_follows_the_session_scratchpad() {
    let explicit = PathBuf::from("/tmp/explicit.md");

    assert_eq!(default_report_path(Some(explicit.clone())), Some(explicit));
}

#[test]
fn targets_are_listed_with_their_kind() {
    let repo = MiniRepo::build(None);
    let config = Config::load(&repo.root).expect("config loads");
    let mut out = Vec::new();

    print_targets(&mut out, &repo.root, &detect_targets(&repo.root, &config));

    let printed = String::from_utf8(out).expect("utf8");
    assert!(printed.contains("targets under"));
    assert!(printed.contains("workspace"));
    assert!(printed.contains("member"));
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
    assert!(out.contains("verdict: SHIP-READY"));
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
    assert!(out.contains("FAILURE REPORT"));
    assert!(out.contains("gate 4/6 — tests — FAIL"));
    assert!(err.contains("error: guardrails BLOCKED"));
    assert!(std::fs::read_to_string(&report)
        .expect("report")
        .contains("VERDICT: BLOCKED — tests=FAIL"));
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

#[test]
fn a_check_that_never_ran_is_reported_as_not_ship_ready() {
    let repo = MiniRepo::build(None);
    let config = Config::load(&repo.root).expect("config loads");
    let session = Session {
        repo: repo.root.clone(),
        config,
        base: "HEAD".to_string(),
        changed: vec!["README.md".to_string()],
        revision: "abc".to_string(),
        dirty: "def".to_string(),
        gates: vec!["tests".to_string()],
        scratch: repo.root.join("scratch"),
        baseline_lcov: None,
        report_path: None,
        apply_aid: false,
        as_json: false,
        selection: "auto".to_string(),
    };
    let targets = detect_targets(&repo.root, &session.config);
    let mut out = Vec::new();

    let selected = select_targets(&parse_from(&["--repo", "."]), &session, &targets, &mut out)
        .expect("selection is fine");

    assert!(selected.is_none());
    assert!(String::from_utf8_lossy(&out).contains("nothing to measure"));
}

#[test]
fn every_target_can_be_selected_with_all() {
    let repo = MiniRepo::build(None);
    let config = Config::load(&repo.root).expect("config loads");
    let session = Session {
        repo: repo.root.clone(),
        config,
        base: "HEAD".to_string(),
        changed: vec!["lib/src/foo.rs".to_string()],
        revision: "abc".to_string(),
        dirty: "def".to_string(),
        gates: vec!["tests".to_string()],
        scratch: repo.root.join("scratch"),
        baseline_lcov: None,
        report_path: None,
        apply_aid: false,
        as_json: false,
        selection: "auto".to_string(),
    };
    let targets = detect_targets(&repo.root, &session.config);

    let selected = select_targets(&parse_from(&["--all"]), &session, &targets, &mut Vec::new())
        .expect("selection is fine")
        .expect("targets selected");

    assert!(selected.iter().any(|target| target.name == "workspace"));
    assert!(selected.iter().any(|target| target.name == "frontend"));
}

#[test]
fn a_pass_verdict_line_matches_the_gate_result() {
    let repo = MiniRepo::build(None);
    let session = Session {
        repo: repo.root.clone(),
        config: Config::load(&repo.root).expect("config loads"),
        base: "HEAD".to_string(),
        changed: Vec::new(),
        revision: "abc".to_string(),
        dirty: "def".to_string(),
        gates: Vec::new(),
        scratch: repo.root.join("scratch"),
        baseline_lcov: None,
        report_path: None,
        apply_aid: false,
        as_json: false,
        selection: "auto".to_string(),
    };
    let mut out = Vec::new();

    print_verdict(
        &mut out,
        &session,
        &Target::workspace_target(),
        &[GateResult::new(
            "tests",
            PASS,
            "3 passed",
            Vec::<String>::new(),
        )],
    );

    assert!(String::from_utf8(out)
        .expect("utf8")
        .contains("verdict: SHIP-READY"));
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

#[test]
fn the_session_records_how_the_target_was_chosen() {
    let repo = MiniRepo::build(None);
    let config = Config::load(&repo.root).expect("config loads");
    let runner = FakeRunner::default();

    let auto = build_session(
        &parse_from(&["--repo", "."]),
        &repo.root,
        config.clone(),
        &runner,
    );
    assert_eq!(auto.selection, "auto");

    let requested = build_session(
        &parse_from(&["--repo", ".", "frontend"]),
        &repo.root,
        config,
        &runner,
    );
    assert_eq!(requested.selection, "requested");
}
