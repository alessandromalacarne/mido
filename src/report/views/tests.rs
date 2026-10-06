use super::*;
use crate::report::FAIL;
use crate::test_support::sample_results;

#[test]
fn failure_report_gives_contract_evidence_and_fix() {
    let report = render_failure(
        &sample_results(),
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
        report.contains("`.mido.toml` [size] file_loc.fail = 500"),
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
        &sample_results(),
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
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            selected_how: "",
            source_label: "rust",
            source_count: 1,
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
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            selected_how: "auto",
            source_label: "rust",
            source_count: 1,
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
    assert!(banner.ends_with("lib/src/foo.rs"), "files follow the panel");
}

#[test]
fn the_panels_shorten_the_hashes_for_the_eye() {
    let banner = render_banner(
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            revision: "fa5bac3800000000000000000000000000000000",
            dirty: "abc123000000000000000000000000000000000000",
            ..BannerContext::default()
        },
        Style::plain(),
    );
    let failure = render_failure(
        &sample_results(),
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
fn banner_counts_the_rust_files_and_caps_the_listing() {
    let changed: Vec<String> = (0..25).map(|index| format!("src/file{index}.rs")).collect();

    let banner = render_banner(
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            base: "HEAD",
            changed: &changed,
            source_label: "rust",
            source_count: changed.len(),
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
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            base: "HEAD",
            changed: &changed,
            source_label: "rust",
            source_count: changed.len(),
            ..BannerContext::default()
        },
        Style::plain(),
    );

    assert!(!banner.contains("more"));
    assert!(banner.contains(&format!("src/file{}.rs", MAX_BANNER_FILES - 1)));

    let mut one_more = changed;
    one_more.push("src/over.rs".to_string());
    let banner = render_banner(
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            base: "HEAD",
            changed: &one_more,
            source_label: "rust",
            source_count: one_more.len(),
            ..BannerContext::default()
        },
        Style::plain(),
    );

    assert!(banner.contains("and 1 more"));
    assert!(!banner.contains("src/over.rs"));
}
