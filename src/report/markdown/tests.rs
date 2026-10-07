use super::*;
use crate::report::{GateResult, PASS};
use crate::targets::Scope;
use crate::test_support::sample_results;

#[test]
fn the_report_spells_out_the_fix_for_a_failing_gate() {
    let report = render_report_markdown(
        &sample_results(),
        &ReportContext {
            target: &Target::workspace_target("Cargo.toml"),
            scope: Scope::Diff,
            base: "origin/mvp",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
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
fn a_report_without_a_base_names_the_explicit_paths() {
    let report = render_report_markdown(
        &[GateResult::new(
            "tests",
            PASS,
            "1 passed",
            Vec::<String>::new(),
        )],
        &ReportContext {
            target: &Target::workspace_target("Cargo.toml"),
            scope: Scope::Paths,
            base: "",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            source_label: "rust",
            source_count: 1,
        },
    );

    assert!(report.contains("scope: `explicit paths`"), "{report}");
    assert!(!report.contains("base:"), "{report}");
}

#[test]
fn a_whole_target_report_names_the_scope_and_the_measured_files() {
    let report = render_report_markdown(
        &[GateResult::new(
            "tests",
            PASS,
            "1 passed",
            Vec::<String>::new(),
        )],
        &ReportContext {
            target: &Target::workspace_target("Cargo.toml"),
            scope: Scope::Whole,
            base: "",
            revision: "fa5bac38",
            dirty: "abc123",
            changed: &["lib/src/foo.rs".to_string()],
            source_label: "rust",
            source_count: 1,
        },
    );

    assert!(
        report.contains("scope: `whole target (no diff)`"),
        "{report}"
    );
    assert!(report.contains("measured files: 1 (1 rust)"), "{report}");
    assert!(!report.contains("changed files:"), "{report}");
}
