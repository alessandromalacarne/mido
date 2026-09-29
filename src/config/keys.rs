use std::collections::BTreeSet;

pub const GATES: [&str; 6] = [
    "syntax", "size", "analysis", "tests", "coverage", "mutation",
];

pub const EXTRA_TOP_LEVEL_KEYS: [&str; 3] = ["version", "targets", "script"];
pub const FAILURE_KEYS: [&str; 1] = ["max_attempts_per_gate"];
pub const TARGET_KEYS: [&str; 3] = ["path", "scope", "manifest"];
pub const THRESHOLD_KEYS: [&str; 2] = ["warn", "fail"];

pub fn gate_keys(gate: &str) -> &'static [&'static str] {
    match gate {
        "syntax" => &["enabled", "command", "format", "lint", "typecheck"],
        "size" => &[
            "enabled",
            "command",
            "tool",
            "file_loc",
            "function_loc",
            "complexity",
            "nesting",
        ],
        "analysis" => &[
            "enabled",
            "command",
            "tool",
            "mi_min",
            "cognitive_max",
            "halstead_effort_max",
            "duplication_command",
        ],
        "tests" => &["enabled", "command", "timeout_secs"],
        "coverage" => &["enabled", "command", "changed_file_min", "total_drop_max"],
        "mutation" => &[
            "enabled",
            "command",
            "scope",
            "timeout_secs",
            "kill_rate_min",
        ],
        _ => &[],
    }
}

pub fn section_keys(section: &str) -> BTreeSet<&'static str> {
    if section == "failure" {
        return FAILURE_KEYS.into_iter().collect();
    }
    gate_keys(section).iter().copied().collect()
}

pub fn top_level_keys() -> BTreeSet<&'static str> {
    let mut keys: BTreeSet<&'static str> = EXTRA_TOP_LEVEL_KEYS.into_iter().collect();
    keys.insert("failure");
    keys.extend(GATES);
    keys
}

pub fn target_keys() -> BTreeSet<&'static str> {
    let mut keys: BTreeSet<&'static str> = TARGET_KEYS.into_iter().collect();
    keys.extend(GATES);
    keys
}

/// Key names as they appear in `.guardrails.toml` for a nested table value.
pub fn joined(keys: impl IntoIterator<Item = &'static str>) -> String {
    keys.into_iter().collect::<Vec<_>>().join(", ")
}
