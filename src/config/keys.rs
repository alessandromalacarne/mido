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
            "tool",
            "file_loc",
            "function_loc",
            "complexity",
            "nesting",
        ],
        "analysis" => &["enabled", "tool", "mi_min", "cognitive_max"],
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

/// Key names as they appear in `.guardrails.toml` for a nested table value.
pub fn joined(keys: impl IntoIterator<Item = &'static str>) -> String {
    keys.into_iter().collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gate_names_its_own_keys() {
        for gate in GATES {
            assert!(!gate_keys(gate).is_empty(), "{gate} has keys");
            assert!(gate_keys(gate).contains(&"enabled"));
        }
    }

    #[test]
    fn command_keys_are_gate_keys() {
        for gate in GATES {
            for key in command_keys(gate) {
                assert!(gate_keys(gate).contains(key), "{gate} accepts {key}");
            }
        }
    }

    #[test]
    fn top_level_keys_are_the_sections_plus_the_scalars() {
        let keys = top_level_keys();

        assert!(keys.contains("script"));
        assert!(keys.contains("targets"));
        assert!(keys.contains("version"));
        assert!(keys.contains("syntax"));
        assert!(keys.contains("analysis"));
        assert!(keys.contains("failure"));
        assert!(!keys.contains("max_attempts_per_gate"));
        assert!(!keys.contains("min_mi"));
    }

    #[test]
    fn target_keys_accept_the_layout_keys_and_the_gates() {
        let keys = target_keys();

        assert!(keys.contains("path"));
        assert!(keys.contains("scope"));
        assert!(keys.contains("manifest"));
        assert!(keys.contains("tests"));
    }
}
