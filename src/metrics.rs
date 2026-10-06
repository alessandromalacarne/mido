//! Readings the gates judge: function units and lcov reports.

use crate::targets::Target;
use std::collections::BTreeMap;
use std::path::Path;

/// One measured function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub path: String,
    pub name: String,
    pub sloc: i64,
    pub cyclomatic: i64,
    pub cognitive: i64,
    pub nesting: Option<i64>,
    pub mi: f64,
}

impl Unit {
    pub fn new(path: &str, name: &str) -> Self {
        Self {
            path: path.to_string(),
            name: name.to_string(),
            sloc: 0,
            cyclomatic: 0,
            cognitive: 0,
            nesting: None,
            mi: 100.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LcovStat {
    pub lines_found: i64,
    pub lines_hit: i64,
}

pub fn lcov_files(path: &Path) -> BTreeMap<String, LcovStat> {
    let mut files = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return files;
    };

    let mut current: Option<String> = None;
    for line in text.lines() {
        apply_lcov_line(line, &mut current, &mut files);
    }
    files
}

/// One `SF:`/`LF:`/`LH:` line: a new source file, or a count for the current one.
fn apply_lcov_line(
    line: &str,
    current: &mut Option<String>,
    files: &mut BTreeMap<String, LcovStat>,
) {
    if let Some(source) = line.strip_prefix("SF:") {
        files.insert(source.to_string(), LcovStat::default());
        *current = Some(source.to_string());
    } else if let Some(found) = line.strip_prefix("LF:") {
        if let Some(stat) = stat_for(current, files) {
            stat.lines_found = found.trim().parse().unwrap_or_default();
        }
    } else if let Some(hit) = line.strip_prefix("LH:") {
        if let Some(stat) = stat_for(current, files) {
            stat.lines_hit = hit.trim().parse().unwrap_or_default();
        }
    }
}

fn stat_for<'a>(
    current: &Option<String>,
    files: &'a mut BTreeMap<String, LcovStat>,
) -> Option<&'a mut LcovStat> {
    files.get_mut(current.as_ref()?)
}

pub fn percent(hit: i64, found: i64) -> f64 {
    if found == 0 {
        0.0
    } else {
        100.0 * hit as f64 / found as f64
    }
}

pub fn relative_to(path: &str, target: &Target) -> String {
    let marker = if target.path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", target.path)
    };
    match path.find(&marker) {
        Some(index) => path[index + marker.len()..].to_string(),
        None => path.to_string(),
    }
}

pub fn touches(diagnostic_path: &str, changed: &[String]) -> bool {
    changed
        .iter()
        .any(|path| diagnostic_path.ends_with(path.as_str()) || path.ends_with(diagnostic_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target::crate_target("frontend", false, "Cargo.toml")
    }

    #[test]
    fn lcov_counts_each_source_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let report = directory.path().join("lcov.info");
        std::fs::write(
            &report,
            "SF:/repo/lib/src/foo.rs\nLF:10\nLH:2\nend_of_record\nSF:/repo/lib/src/bar.rs\nLF:4\nLH:4\nend_of_record\n",
        )
        .expect("report");

        let files = lcov_files(&report);

        assert_eq!(files.len(), 2);
        assert_eq!(
            files["/repo/lib/src/foo.rs"],
            LcovStat {
                lines_found: 10,
                lines_hit: 2
            }
        );
        assert_eq!(
            files["/repo/lib/src/bar.rs"],
            LcovStat {
                lines_found: 4,
                lines_hit: 4
            }
        );
    }

    #[test]
    fn a_missing_report_measures_nothing() {
        assert!(lcov_files(Path::new("/nonexistent/lcov.info")).is_empty());
    }

    #[test]
    fn percent_handles_the_empty_denominator() {
        assert_eq!(percent(0, 0), 0.0);
        assert_eq!(percent(2, 10), 20.0);
    }

    #[test]
    fn paths_are_shown_relative_to_the_target() {
        assert_eq!(
            relative_to("/repo/frontend/src/main.rs", &target()),
            "src/main.rs"
        );
        assert_eq!(
            relative_to(
                "/repo/frontend/src/main.rs",
                &Target::workspace_target("Cargo.toml")
            ),
            "repo/frontend/src/main.rs"
        );
        assert_eq!(relative_to("src/main.rs", &target()), "src/main.rs");
    }

    #[test]
    fn a_diagnostic_touches_a_changed_file_in_either_direction() {
        let changed = vec!["src/main.rs".to_string()];

        assert!(touches("src/main.rs", &changed));
        assert!(touches("/abs/path/src/main.rs", &changed));
        assert!(!touches("src/other.rs", &changed));
    }
}
