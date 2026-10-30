use super::*;
use crate::test_support::argv;
use std::path::PathBuf;

#[test]
fn the_report_directory_flag_decides_the_report_path() {
    for command in [
        argv(&[
            "npx",
            "vitest",
            "run",
            "--coverage.reportsDirectory=reports",
        ]),
        argv(&["npx", "jest", "--coverageDirectory=reports"]),
        argv(&["npx", "c8", "--reports-dir", "reports", "npm", "test"]),
        argv(&["npx", "c8", "--report-dir", "reports", "npm", "test"]),
    ] {
        assert_eq!(
            report_path(&command),
            Some(PathBuf::from("reports").join("lcov.info"))
        );
    }
}
