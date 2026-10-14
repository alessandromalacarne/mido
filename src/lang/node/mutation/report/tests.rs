use super::*;

/// The report's file entries, as the json reporter writes them.
fn write_report(dir: &Path, files: serde_json::Value) {
    std::fs::create_dir_all(dir).expect("dir");
    std::fs::write(
        report_path(dir),
        serde_json::json!({ "files": files }).to_string(),
    )
    .expect("report");
}

fn mutants(statuses: &[&str]) -> serde_json::Value {
    serde_json::json!(statuses
        .iter()
        .enumerate()
        .map(|(index, status)| serde_json::json!({
            "id": index,
            "status": status,
            "mutatorName": "ArithmeticOperator",
            "replacement": "x",
            "location": { "start": { "line": 2, "column": 20 } },
        }))
        .collect::<Vec<_>>())
}

fn read_statuses(statuses: &[&str]) -> Option<MutationReport> {
    let dir = tempfile::tempdir().expect("temp dir");
    write_report(
        dir.path(),
        serde_json::json!({ "src/calc.js": { "mutants": mutants(statuses) } }),
    );
    read(dir.path())
}

#[test]
fn every_status_is_counted_where_it_belongs() {
    let report = read_statuses(&[
        "Killed",
        "Killed",
        "Survived",
        "NoCoverage",
        "Timeout",
        "CompileError",
    ])
    .expect("report");

    assert_eq!(report.total, 6);
    assert_eq!(report.caught, 2);
    assert_eq!(report.missed, 2, "no-coverage mutants are survivors too");
    assert_eq!(report.timeout, 1);
    assert_eq!(report.unviable, 1);
    assert_eq!(
        report.survivors,
        vec!["src/calc.js:2:20 ArithmeticOperator -> x"; 2]
    );
    assert_eq!(report.timed_out.len(), 1);
    assert_eq!(report.baseline_failure, None);
}

#[test]
fn ignored_mutants_stay_out_of_the_total() {
    let report = read_statuses(&["Killed", "Ignored", "Ignored"]).expect("report");

    assert_eq!(report.total, 1);
    assert_eq!(report.caught, 1);
}

#[test]
fn a_status_without_a_verdict_keeps_its_mutant_in_the_total() {
    let report = read_statuses(&["Killed", "Pending", "RuntimeError"]).expect("report");

    assert_eq!(report.total, 3, "no verdict is not a silent drop");
    assert_eq!(report.caught, 1);
    assert_eq!(report.missed, 0);
    assert_eq!(report.unviable, 0);
}

#[test]
fn a_missing_report_is_not_read() {
    let dir = tempfile::tempdir().expect("temp dir");

    assert_eq!(read(dir.path()), None);
}

#[test]
fn a_report_whose_shape_changed_is_not_read() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(report_path(dir.path()), "{ \"mutants\": [] }").expect("report");

    assert_eq!(
        read(dir.path()),
        None,
        "a field the reader needs must never default to zero"
    );
}
