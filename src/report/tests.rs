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
fn the_verdict_panel_paints_by_outcome() {
    let shipped = render_verdict(SHIP_READY, Style::colored());
    let blocked = render_verdict("BLOCKED — size=FAIL", Style::colored());

    assert!(
        shipped.contains("\u{1b}[32mSHIP-READY\u{1b}[0m"),
        "{shipped:?}"
    );
    assert!(
        blocked.contains("\u{1b}[31mBLOCKED — size=FAIL\u{1b}[0m"),
        "{blocked:?}"
    );
}

#[test]
fn failure_report_gives_contract_evidence_and_fix() {
    let report = render_failure(
        &results(),
        &FailureContext {
            target: "frontend",
            revision: "fa5bac38",
            dirty: "abc123",
            base: "origin/mvp",
            attempts: None,
        },
        Style::plain(),
    );

    assert!(report.contains("╭─ failure"), "{report}");
    assert!(report.contains("│ BLOCKED — size=FAIL"), "{report}");
    assert!(report.contains("│ target   frontend"), "{report}");
    assert!(report.contains("│ base     origin/mvp"), "{report}");
    assert!(report.contains("│ failing  1 of 3 gates"), "{report}");
    assert!(report.contains("[2/6] ✗ size — FAIL"), "{report}");
    assert!(
        report.contains("`.guardrails.toml` [size] file_loc.fail = 500"),
        "{report}"
    );
    assert!(
        report.contains("src/components/task/creator.rs: 512 code lines"),
        "{report}"
    );
    assert!(
        report.contains("split `create_task` along a real seam"),
        "{report}"
    );
    assert!(
        !report.contains("syntax"),
        "a gate that passed is not in the report"
    );
}

#[test]
fn failure_evidence_does_not_repeat_the_summary() {
    let report = render_failure(
        &[GateResult::new(
            "tests",
            FAIL,
            "cargo test: 2 passed, 1 failed",
            [
                "cargo test: 2 passed, 1 failed".to_string(),
                "`cargo test`: 1 failed -> fails".to_string(),
            ],
        )],
        &FailureContext::default(),
        Style::plain(),
    );

    assert_eq!(
        report.matches("cargo test: 2 passed, 1 failed").count(),
        1,
        "{report}"
    );
    assert!(
        report.contains("`cargo test`: 1 failed -> fails"),
        "{report}"
    );
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
        Style::plain(),
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
        Style::plain(),
    );

    assert!(report.contains("max 3 distinct hypotheses"));
}

#[test]
fn banner_reports_the_revision_it_measured() {
    let banner = render_banner(
        &Target::workspace_target(),
        &BannerContext {
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            selected_how: "",
            runner: "",
        },
        Style::plain(),
    );

    assert!(banner.contains("workspace"));
    assert!(banner.contains("fa5bac38"));
    assert!(banner.contains("abc123"));
    assert!(banner.contains("lib/src/foo.rs"));
}

#[test]
fn the_banner_is_a_panel_of_labelled_fields() {
    let banner = render_banner(
        &Target::workspace_target(),
        &BannerContext {
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            selected_how: "auto",
            runner: "scripts/guardrails.py",
        },
        Style::plain(),
    );

    assert!(banner.starts_with("╭─ mido ─"), "{banner}");
    assert!(
        banner.contains("│ target   workspace (./) [auto]"),
        "{banner}"
    );
    assert!(banner.contains("│ base     origin/mvp"), "{banner}");
    assert!(banner.contains("│ revision fa5bac38"), "{banner}");
    assert!(banner.contains("│ changed  1 files (1 rust)"), "{banner}");
    assert!(
        banner.contains("script"),
        "the runner row keeps the declaration note"
    );
    assert!(banner.ends_with("lib/src/foo.rs"), "files follow the panel");
}

#[test]
fn a_panel_pads_painted_rows_by_their_visible_width() {
    let printed = panel(
        "verdict",
        &[Style::colored().pass("SHIP-READY")],
        Style::colored(),
    );

    assert!(
        printed.contains("│ \u{1b}[32mSHIP-READY\u{1b}[0m │"),
        "the row closes flush with the rule: {printed:?}"
    );
    assert!(
        printed.lines().all(|line| visible_len(line) == 14),
        "{printed:?}"
    );
}

#[test]
fn a_panel_is_never_narrower_than_its_title() {
    let printed = panel("failure", &[], Style::plain());

    assert_eq!(printed, "╭─ failure ╮\n╰──────────╯");
}

#[test]
fn the_panels_shorten_the_hashes_for_the_eye() {
    let banner = render_banner(
        &Target::workspace_target(),
        &BannerContext {
            revision: "fa5bac3800000000000000000000000000000000",
            dirty: "abc123000000000000000000000000000000000000",
            ..BannerContext::default()
        },
        Style::plain(),
    );
    let failure = render_failure(
        &results(),
        &FailureContext {
            revision: "fa5bac3800000000000000000000000000000000",
            ..FailureContext::default()
        },
        Style::plain(),
    );

    assert!(
        banner.contains("fa5bac38") && !banner.contains("fa5bac380"),
        "{banner}"
    );
    assert!(
        failure.contains("fa5bac38") && !failure.contains("fa5bac380"),
        "{failure}"
    );
}

#[test]
fn banner_names_the_runner_the_config_declares() {
    let banner = render_banner(
        &Target::workspace_target(),
        &BannerContext {
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            selected_how: "auto",
            runner: "scripts/guardrails.py",
        },
        Style::plain(),
    );

    assert!(banner.contains("scripts/guardrails.py"));
    assert!(banner.contains("[auto]"));
}

#[test]
fn banner_counts_the_rust_files_and_caps_the_listing() {
    let changed: Vec<String> = (0..25).map(|index| format!("src/file{index}.rs")).collect();

    let banner = render_banner(
        &Target::workspace_target(),
        &BannerContext {
            base: "HEAD",
            changed: &changed,
            ..BannerContext::default()
        },
        Style::plain(),
    );

    assert!(banner.contains("25 files (25 rust)"));
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
        &BannerContext {
            base: "HEAD",
            changed: &changed,
            ..BannerContext::default()
        },
        Style::plain(),
    );

    assert!(!banner.contains("more"));
    assert!(banner.contains(&format!("src/file{}.rs", MAX_BANNER_FILES - 1)));

    let mut one_more = changed;
    one_more.push("src/over.rs".to_string());
    let banner = render_banner(
        &Target::workspace_target(),
        &BannerContext {
            base: "HEAD",
            changed: &one_more,
            ..BannerContext::default()
        },
        Style::plain(),
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

#[test]
fn a_report_without_a_base_names_the_explicit_paths() {
    let report = render_report_markdown(
        &[GateResult::new(
            "tests",
            PASS,
            "1 passed",
            Vec::<String>::new(),
        )],
        &ReportContext {
            target: &Target::workspace_target(),
            base: "",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            runner: "",
        },
    );

    assert!(report.contains("scope: `explicit paths`"), "{report}");
    assert!(!report.contains("base:"), "{report}");
}
