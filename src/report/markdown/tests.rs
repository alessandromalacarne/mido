use super::*;
use crate::report::{GateResult, PASS};
use crate::test_support::sample_results;

#[test]
fn the_report_spells_out_the_fix_for_a_failing_gate() {
    let report = render_report_markdown(
        &sample_results(),
        &ReportContext {
            target: &Target::workspace_target("Cargo.toml"),
            revision: "fa5bac38",
            dirty: "abc123",
            files: &["lib/src/foo.rs".to_string()],
            source_label: "rust",
            source_count: 1,
        },
    );

    assert!(report.contains("fix:"));
    assert!(report.contains("- split `create_task` along a real seam"));
    assert!(report.contains("| size | FAIL | 2 functions over the ceiling |"));
    assert!(report.contains("contract: `.mido.toml` [size] file_loc.fail = 500"));
}

#[test]
fn the_report_names_the_target_and_the_measured_files() {
    let report = render_report_markdown(
        &[GateResult::new(
            "tests",
            PASS,
            "1 passed",
            Vec::<String>::new(),
        )],
        &ReportContext {
            target: &Target::workspace_target("Cargo.toml"),
            revision: "fa5bac38",
            dirty: "abc123",
            files: &["lib/src/foo.rs".to_string()],
            source_label: "rust",
            source_count: 1,
        },
    );

    assert!(report.contains("VERDICT: SHIP-READY"), "{report}");
    assert!(report.contains("target: `workspace (./)`"), "{report}");
    assert!(report.contains("measured files: 1 (1 rust)"), "{report}");
    assert!(!report.contains("base:"), "{report}");
}
