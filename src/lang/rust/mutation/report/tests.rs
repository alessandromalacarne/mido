use super::*;

/// A finished run's `outcomes.json`, as cargo-mutants v27 writes it: the
/// baseline first, then one entry per mutant tested.
const FINISHED: &str = r#"{
  "outcomes": [
    { "scenario": "Baseline", "summary": "Success", "log_path": "log/baseline.log", "diff_path": null, "phase_results": [] },
    { "scenario": { "Mutant": { "name": "src/foo.rs:1:1: replace + with - in a", "package": "mini", "file": "src/foo.rs" } }, "summary": "CaughtMutant", "log_path": "log/a.log", "diff_path": "diff/a.diff", "phase_results": [] },
    { "scenario": { "Mutant": { "name": "src/foo.rs:2:2: replace a with b in b", "package": "mini", "file": "src/foo.rs" } }, "summary": "MissedMutant", "log_path": "log/b.log", "diff_path": "diff/b.diff", "phase_results": [] },
    { "scenario": { "Mutant": { "name": "src/foo.rs:3:3: replace c with d in c", "package": "mini", "file": "src/foo.rs" } }, "summary": "Timeout", "log_path": "log/c.log", "diff_path": "diff/c.diff", "phase_results": [] }
  ],
  "total_mutants": 3,
  "missed": 1,
  "caught": 1,
  "timeout": 1,
  "unviable": 0,
  "success": 0,
  "start_time": "2026-10-07T04:14:42.291060408Z",
  "end_time": "2026-10-07T04:15:17.119958003Z",
  "cargo_mutants_version": "27.1.0"
}"#;

/// An output parent directory holding the named `mutants.out` files.
fn output_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("mutants.out")).expect("mutants.out");
    for (name, text) in files {
        std::fs::write(dir.path().join("mutants.out").join(name), text).expect("write");
    }
    dir
}

#[test]
fn a_finished_run_is_read() {
    let dir = output_dir(&[
        ("outcomes.json", FINISHED),
        (
            "previously_caught.txt",
            "src/foo.rs:4:4: replace e with f in d\nsrc/foo.rs:5:5: replace g with h in e\n",
        ),
    ]);

    let report = read(dir.path()).expect("report");

    assert_eq!(report.total, 3);
    assert_eq!(report.caught, 1);
    assert_eq!(report.missed, 1);
    assert_eq!(report.unviable, 0);
    assert_eq!(report.timeout, 1);
    assert_eq!(
        report.skipped, 2,
        "the exclusion list is the earlier passes' kills"
    );
    assert_eq!(
        report.survivors,
        vec!["src/foo.rs:2:2: replace a with b in b"],
        "the missed mutants are named"
    );
    assert_eq!(
        report.timed_out,
        vec!["src/foo.rs:3:3: replace c with d in c"],
        "the timed-out mutants are named"
    );
    assert_eq!(report.baseline_failure, None);
}

#[test]
fn a_run_with_no_exclusion_list_skips_nothing() {
    let dir = output_dir(&[("outcomes.json", FINISHED)]);

    let report = read(dir.path()).expect("report");

    assert_eq!(report.skipped, 0, "a fresh pass excludes nothing");
}

#[test]
fn a_run_that_did_not_finish_is_not_read() {
    let unfinished = FINISHED.replace(
        "\"end_time\": \"2026-10-07T04:15:17.119958003Z\"",
        "\"end_time\": null",
    );
    let dir = output_dir(&[("outcomes.json", &unfinished)]);

    assert_eq!(
        read(dir.path()),
        None,
        "a run still going (or killed) has no verdict to read"
    );
}

#[test]
fn a_missing_report_is_not_read() {
    let dir = output_dir(&[]);

    assert_eq!(read(dir.path()), None);
}

#[test]
fn a_report_whose_shape_changed_is_not_read() {
    let renamed = FINISHED.replace("\"caught\": 1", "\"caught_mutants\": 1");
    let dir = output_dir(&[("outcomes.json", &renamed)]);

    assert_eq!(
        read(dir.path()),
        None,
        "a field the reader needs must never default to zero"
    );
}

#[test]
fn a_failed_baseline_is_reported() {
    let failed = FINISHED.replace(
        "{ \"scenario\": \"Baseline\", \"summary\": \"Success\"",
        "{ \"scenario\": \"Baseline\", \"summary\": \"Failure\"",
    );
    let dir = output_dir(&[("outcomes.json", &failed)]);

    let report = read(dir.path()).expect("report");

    assert_eq!(report.baseline_failure.as_deref(), Some("Failure"));
}
