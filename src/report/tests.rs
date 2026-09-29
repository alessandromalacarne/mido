use super::*;

fn results() -> Vec<GateResult> {
    vec![
        GateResult::new("syntax", PASS, "clean", Vec::<String>::new())
            .contract("`.guardrails.toml` [syntax] fmt/lint/typecheck"),
        GateResult::new(
            "size",
            FAIL,
            "2 functions over the ceiling",
            [
                "file src/components/task/creator.rs: 512 code lines (FAIL, fail >= 500)"
                    .to_string(),
                "function create_task (src/components/task/creator.rs): 71 sloc -> function_loc"
                    .to_string(),
            ],
        )
        .contract("`.guardrails.toml` [size] file_loc.fail = 500")
        .fixes(["split `create_task` along a real seam"]),
        GateResult::new("tests", PASS, "412 passed, 0 failed", Vec::<String>::new())
            .contract("`.guardrails.toml` [tests] command"),
    ]
}

#[test]
fn exit_codes_separate_blocked_from_incomplete() {
    assert_eq!(exit_code(&results()[..1]), 0);
    assert_eq!(exit_code(&results()), 1);
    assert_eq!(
        exit_code(&[GateResult::new(
            "coverage",
            INCOMPLETE,
            "no report",
            Vec::<String>::new()
        )]),
        2
    );
    assert_eq!(
        exit_code(&[GateResult::new(
            "mutation",
            FAIL,
            "40% killed",
            Vec::<String>::new()
        )]),
        1
    );
}

#[test]
fn verdict_names_every_gate_that_did_not_pass() {
    let mut all = results();
    all.push(GateResult::new(
        "mutation",
        INCOMPLETE,
        "timed out",
        Vec::<String>::new(),
    ));

    let verdict = verdict(&all);

    assert!(verdict.starts_with("BLOCKED"));
    assert!(verdict.contains("size=FAIL"));
    assert!(verdict.contains("mutation=INCOMPLETE"));
}

#[test]
fn verdict_is_ship_ready_only_when_every_gate_passes() {
    assert_eq!(verdict(&results()[..1]), "SHIP-READY");
}

#[test]
fn failure_report_gives_contract_evidence_and_fix() {
    let report = render_failure(
        &results(),
        &FailureContext {
            target: "frontend",
            revision: "fa5bac38",
            dirty: "abc123",
            base: "",
            attempts: None,
        },
    );

    assert!(report.contains("size"));
    assert!(report.contains("`.guardrails.toml` [size] file_loc.fail = 500"));
    assert!(report.contains("src/components/task/creator.rs: 512 code lines"));
    assert!(report.contains("split `create_task` along a real seam"));
    assert!(report.contains("frontend"));
    assert!(report.contains("FAILURE REPORT — BLOCKED — size=FAIL"));
    assert!(report.contains("failing  1 of 3 gate(s)"));
    assert!(!report.contains("syntax"));
}

#[test]
fn failure_report_flags_incomplete_gates_as_not_passed() {
    let report = render_failure(
        &[GateResult::new(
            "mutation",
            INCOMPLETE,
            "cargo-mutants produced no summary",
            Vec::<String>::new(),
        )],
        &FailureContext::default(),
    );

    assert!(report.contains("mutation"));
    assert!(report.contains("INCOMPLETE"));
    assert!(report.contains("never a pass"));
}

#[test]
fn failure_report_records_the_attempt_cap_when_the_config_has_one() {
    let report = render_failure(
        &results(),
        &FailureContext {
            attempts: Some(3),
            ..FailureContext::default()
        },
    );

    assert!(report.contains("max 3 distinct hypotheses"));
}

#[test]
fn banner_reports_the_revision_it_measured() {
    let banner = render_banner(
        &Target::workspace_target(),
        "origin/mvp",
        "fa5bac38",
        "abc123",
        &["lib/src/foo.rs".to_string()],
        "",
        "",
    );

    assert!(banner.contains("workspace"));
    assert!(banner.contains("fa5bac38"));
    assert!(banner.contains("abc123"));
    assert!(banner.contains("lib/src/foo.rs"));
}

#[test]
fn banner_names_the_runner_the_config_declares() {
    let banner = render_banner(
        &Target::workspace_target(),
        "origin/mvp",
        "fa5bac38",
        "abc123",
        &["lib/src/foo.rs".to_string()],
        "auto",
        "scripts/guardrails.py",
    );

    assert!(banner.contains("scripts/guardrails.py"));
    assert!(banner.contains("[auto]"));
}

#[test]
fn banner_counts_the_rust_files_and_caps_the_listing() {
    let changed: Vec<String> = (0..25).map(|index| format!("src/file{index}.rs")).collect();

    let banner = render_banner(
        &Target::workspace_target(),
        "HEAD",
        "",
        "",
        &changed,
        "",
        "",
    );

    assert!(banner.contains("changed files: 25 (25 rust)"));
    assert!(banner.contains("and 5 more"));
    assert!(banner.contains("revision unknown"));
}

#[test]
fn a_listing_of_exactly_the_cap_is_not_truncated() {
    let changed: Vec<String> = (0..MAX_BANNER_FILES)
        .map(|index| format!("src/file{index}.rs"))
        .collect();

    let banner = render_banner(
        &Target::workspace_target(),
        "HEAD",
        "",
        "",
        &changed,
        "",
        "",
    );

    assert!(!banner.contains("more"));
    assert!(banner.contains(&format!("src/file{}.rs", MAX_BANNER_FILES - 1)));

    let mut one_more = changed;
    one_more.push("src/over.rs".to_string());
    let banner = render_banner(
        &Target::workspace_target(),
        "HEAD",
        "",
        "",
        &one_more,
        "",
        "",
    );

    assert!(banner.contains("and 1 more"));
    assert!(!banner.contains("src/over.rs"));
}

#[test]
fn the_report_spells_out_the_fix_for_a_failing_gate() {
    let report = render_report_markdown(
        &results(),
        &ReportContext {
            target: &Target::workspace_target(),
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            runner: "",
        },
    );

    assert!(report.contains("fix:"));
    assert!(report.contains("- split `create_task` along a real seam"));
    assert!(report.contains("| size | FAIL | 2 functions over the ceiling |"));
    assert!(report.contains("contract: `.guardrails.toml` [size] file_loc.fail = 500"));
}

#[test]
fn report_records_the_runner_the_config_declares() {
    let report = render_report_markdown(
        &[GateResult::new(
            "tests",
            PASS,
            "1 passed",
            Vec::<String>::new(),
        )],
        &ReportContext {
            target: &Target::workspace_target(),
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            runner: "scripts/guardrails.py",
        },
    );

    assert!(report.contains("runner: `scripts/guardrails.py`"));
    assert!(report.contains("| tests | PASS | 1 passed |"));
    assert!(report.contains("VERDICT: SHIP-READY"));
}
