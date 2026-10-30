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
            attempts: None,
        },
        Style::plain(),
    );

    assert!(report.contains("╭─ failure"), "{report}");
    assert!(report.contains("│ BLOCKED — size=FAIL"), "{report}");
    assert!(report.contains("│ target   frontend"), "{report}");
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
            "cargo-mutants produced no report",
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
fn a_listing_of_exactly_the_cap_is_not_truncated() {
    let files: Vec<String> = (0..MAX_BANNER_FILES)
        .map(|index| format!("src/file{index}.rs"))
        .collect();

    let banner = render_banner(
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            files: &files,
            source_label: "rust",
            source_count: files.len(),
            ..BannerContext::default()
        },
        Style::plain(),
    );

    assert!(!banner.contains("more"));
    assert!(banner.contains(&format!("src/file{}.rs", MAX_BANNER_FILES - 1)));

    let mut one_more = files;
    one_more.push("src/over.rs".to_string());
    let banner = render_banner(
        &Target::workspace_target("Cargo.toml"),
        &BannerContext {
            files: &one_more,
            source_label: "rust",
            source_count: one_more.len(),
            ..BannerContext::default()
        },
        Style::plain(),
    );

    assert!(banner.contains("and 1 more"));
    assert!(!banner.contains("src/over.rs"));
}
