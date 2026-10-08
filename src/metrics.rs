//! Readings the gates judge: function units and lcov reports.

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

