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
        Lang::Rust,
    );

    assert!(command.join(" ").contains("guardrails-lcov-workspace.info"));
    assert_eq!(path, scratch.join("guardrails-lcov-workspace.info"));
}

#[test]
fn a_missing_report_is_incomplete_not_a_pass() {
    let repo = repo();
    let runner = FakeRunner::with(&[("llvm-cov", 101, "error: no such command")]);
    let files = vec!["lib/src/foo.rs".to_string()];
    let result = gate_coverage(&runner, &gate_run_for(&repo, &config_for(&repo), &files));

    assert_eq!(result.status, INCOMPLETE);
    assert!(result
        .details
        .iter()
        .any(|line| line.contains("exited 101")));
    assert!(result.fixes[0].contains("writes"));
}

#[test]
fn a_total_drop_beyond_the_allowance_fails_the_gate() {
    let repo = repo();
    write_report(
        &repo.root.join("lcov.info"),
        "SF:/repo/lib/src/foo.rs\nLH:9\nLF:10\nend_of_record\n",
    );
    let baseline = repo.root.join("baseline.info");
    write_report(
        &baseline,
        "SF:/repo/lib/src/foo.rs\nLH:10\nLF:10\nend_of_record\n",
    );
    let runner = FakeRunner::with(&[("llvm-cov", 0, "")]);
    let files = vec!["lib/src/foo.rs".to_string()];
    let config = config_for(&repo);
    let mut run = gate_run_for(&repo, &config, &files);
    run.baseline_lcov = Some(&baseline);
    let result = gate_coverage(&runner, &run);

    assert_eq!(result.status, FAIL);
    assert!(result
        .details
        .join(" ")
        .contains("coverage dropped -10.0 points"));
}
