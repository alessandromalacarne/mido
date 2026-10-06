use super::*;
use crate::test_support::{argv, config_for, gate_run_for, FakeRunner, MiniRepo};

fn repo() -> MiniRepo {
    MiniRepo::build(None)
}

fn write_report(path: &Path, body: &str) {
    std::fs::write(path, body).expect("report");
}

#[test]
fn the_report_path_is_filled_in_and_located() {
    let repo = repo();
    let scratch = repo.root.join("scratch");

    let (command, path) = coverage_report_path(
        &argv(&["cargo", "llvm-cov", "--lcov", "--output-path", "{lcov}"]),
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &scratch,
    );

    assert!(command.join(" ").contains("guardrails-lcov-workspace.info"));
    assert_eq!(path, scratch.join("guardrails-lcov-workspace.info"));
}

#[test]
fn a_relative_report_path_lands_in_the_target_directory() {
    let repo = repo();

    let (_, path) = coverage_report_path(
        &argv(&["cargo", "llvm-cov", "--lcov", "--output-path", "lcov.info"]),
        &repo.root,
        &Target::crate_target("frontend", false, "Cargo.toml"),
        &repo.root,
    );

    assert_eq!(path, repo.root.join("frontend/lcov.info"));
}

#[test]
fn the_equals_form_of_the_output_flag_is_read_too() {
    let repo = repo();

    let (_, path) = coverage_report_path(
        &argv(&["cargo", "llvm-cov", "--output-path=lcov.info"]),
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &repo.root,
    );

    assert_eq!(path, repo.root.join("lcov.info"));
}

#[test]
fn a_command_without_an_output_flag_defaults_to_lcov_info() {
    let repo = repo();

    let (_, path) = coverage_report_path(
        &argv(&["cargo", "llvm-cov"]),
        &repo.root,
        &Target::workspace_target("Cargo.toml"),
        &repo.root,
    );

    assert_eq!(path, repo.root.join("lcov.info"));
}

#[test]
fn coverage_gate_enforces_the_changed_file_minimum() {
    let repo = repo();
    // The embedded rust baseline writes `lcov.info` next to the target.
    write_report(
        &repo.root.join("lcov.info"),
        "SF:/repo/lib/src/foo.rs\nLH:2\nLF:10\nend_of_record\n",
    );
    let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
    let changed = vec!["lib/src/foo.rs".to_string()];
    let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &changed));

    assert_eq!(result.status, FAIL);
    assert!(result.details.join(" ").contains("20.0%"));
    assert!(result.details.join(" ").contains("min 80"));
}

#[test]
fn a_missing_report_is_incomplete_not_a_pass() {
    let repo = repo();
    let runner = FakeRunner::with(&[("llvm-cov", 101, "error: no such command")]);
    let changed = vec!["lib/src/foo.rs".to_string()];
    let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &changed));

    assert_eq!(result.status, INCOMPLETE);
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("exited 101")));
    assert!(result.fixes[0].contains("writes"));
}

#[test]
fn changed_files_absent_from_the_report_are_called_unverified() {
    let repo = repo();
    write_report(
        &repo.root.join("lcov.info"),
        "SF:/repo/lib/src/other.rs\nLH:10\nLF:10\nend_of_record\n",
    );
    let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
    let changed = vec!["lib/src/foo.rs".to_string()];
    let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &changed));

    assert_eq!(result.status, PASS);
    assert!(result.details.join(" ").contains("unverified, not 100%"));
}

#[test]
fn a_total_drop_beyond_the_allowance_fails_the_gate() {
    let repo = repo();
    write_report(
        &repo.root.join("lcov.info"),
        "SF:/repo/lib/src/foo.rs\nLH:5\nLF:10\nend_of_record\n",
    );
    let baseline = repo.root.join("baseline.info");
    write_report(
        &baseline,
        "SF:/repo/lib/src/foo.rs\nLH:10\nLF:10\nend_of_record\n",
    );
    let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
    let changed = vec!["lib/src/foo.rs".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &changed);
    run.baseline_lcov = Some(&baseline);
    let result = gate_coverage(&runner, &run);

    assert_eq!(result.status, FAIL);
    assert!(result
        .details
        .join(" ")
        .contains("coverage dropped -50.0 points"));
}

#[test]
fn without_a_baseline_the_delta_is_reported_as_not_measured() {
    let (problems, details) = judge_coverage_delta((0, 0), None, 0.0);

    assert!(problems.is_empty());
    assert!(details[0].contains("not measured"));
}

#[test]
fn a_file_missing_from_the_changed_list_is_not_judged() {
    let files = BTreeMap::from([(
        "/repo/lib/src/other.rs".to_string(),
        LcovStat {
            lines_found: 10,
            lines_hit: 1,
        },
    )]);

    let (problems, details) = judge_changed_coverage(
        &files,
        &["lib/src/foo.rs".to_string()],
        &Target::workspace_target("Cargo.toml"),
        80.0,
    );

    assert!(problems.is_empty());
    assert_eq!(details.len(), 1);
}

#[test]
fn changed_files_are_shown_relative_to_the_target() {
    let files = BTreeMap::from([(
        "/repo/frontend/src/main.rs".to_string(),
        LcovStat {
            lines_found: 2,
            lines_hit: 2,
        },
    )]);

    let (problems, details) = judge_changed_coverage(
        &files,
        &["frontend/src/main.rs".to_string()],
        &Target::crate_target("frontend", false, "Cargo.toml"),
        80.0,
    );

    assert!(problems.is_empty());
    assert!(details[0].starts_with("changed file src/main.rs: 2/2 = 100.0% (ok)"));
}
