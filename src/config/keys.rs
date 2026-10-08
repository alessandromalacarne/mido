use crate::gate::Gate;
use std::collections::BTreeSet;

/// The gate section names, in ladder order — the one source for the schema.
pub const GATES: [&str; 6] = [
    Gate::Syntax.name(),
    Gate::Size.name(),
    Gate::Analysis.name(),
    Gate::Tests.name(),
    Gate::Coverage.name(),
    Gate::Mutation.name(),
];

pub const EXTRA_TOP_LEVEL_KEYS: [&str; 2] = ["version", "targets"];
pub const FAILURE_KEYS: [&str; 1] = ["max_attempts_per_gate"];
pub const TARGET_KEYS: [&str; 2] = ["path", "manifest"];
pub const THRESHOLD_KEYS: [&str; 2] = ["warn", "fail"];

pub fn gate_keys(gate: &str) -> &'static [&'static str] {
    match gate {
        "syntax" => &["enabled", "command", "format", "lint", "typecheck"],
        "size" => &[
            "enabled",
            "tool",
            "file_loc",
            "function_loc",
            "complexity",
            "nesting",
        ],
        "analysis" => &["enabled", "tool", "mi_min", "cognitive_max"],
        "tests" => &["enabled", "command", "timeout_secs"],
        "coverage" => &["enabled", "command", "coverage_min", "total_drop_max"],
        "mutation" => &["enabled", "command", "timeout_secs", "kill_rate_min"],
        _ => &[],
    }
}

/// The keys whose value is a command: an argv array of strings.
pub fn command_keys(gate: &str) -> &'static [&'static str] {
    match gate {
        "syntax" => &["command", "format", "lint", "typecheck"],
        "tests" => &["command"],
        "coverage" => &["command"],
        "mutation" => &["command"],
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

/// Key names as they appear in `.mido.toml` for a nested table value.
pub fn joined(keys: impl IntoIterator<Item = &'static str>) -> String {
    keys.into_iter().collect::<Vec<_>>().join(", ")
}

