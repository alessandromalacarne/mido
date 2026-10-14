//! cargo-mutants: its report files, and the test arguments a command carries.

pub mod report;

pub const TOOL: &str = "cargo-mutants";
pub const TIMEOUT_SECS: i64 = 120;

/// The tool's own line for a scope that held no mutant to filter.
const NO_MUTANTS_FILTERED: &str = "No mutants to filter";

/// Whether the run said it found no mutant in the scope it was given.
pub fn found_nothing(output: &str) -> bool {
    output.contains(NO_MUTANTS_FILTERED)
}

/// The arguments a declared test command carries to `cargo test`: everything
/// after the `cargo test` it names. A command that names none carries nothing —
/// the mutation tool's own default test command then applies.
pub fn test_args(command: &[String]) -> Option<Vec<String>> {
    let start = command
        .windows(2)
        .position(|pair| pair[0] == "cargo" && pair[1] == "test")?;

    Some(command[start + 2..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn a_test_command_carries_its_cargo_test_arguments() {
        assert_eq!(
            test_args(&cmd(&["cargo", "test", "--all-features"])),
            Some(cmd(&["--all-features"]))
        );
        assert_eq!(
            test_args(&cmd(&[
                "nix",
                "develop",
                "-c",
                "cargo",
                "test",
                "--target",
                "wasm32-unknown-unknown"
            ])),
            Some(cmd(&["--target", "wasm32-unknown-unknown"]))
        );
    }

    #[test]
    fn a_command_that_names_no_cargo_test_carries_nothing() {
        assert_eq!(test_args(&cmd(&["cargo", "nextest", "run"])), None);
    }

    #[test]
    fn a_run_that_filtered_everything_says_so() {
        assert!(found_nothing("INFO No mutants to filter\n"));
        assert!(!found_nothing("5 mutants tested in 2s: 5 missed"));
    }
}
