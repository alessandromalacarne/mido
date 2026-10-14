//! cargo-mutants: its pass argv, its report files, and the test arguments a
//! command carries.

pub mod report;

use crate::error::GuardrailsError;
use std::path::Path;

pub const TOOL: &str = "cargo-mutants";
pub const TIMEOUT_SECS: i64 = 120;

/// The tool's own line for a scope that held no mutant to filter.
const NO_MUTANTS_FILTERED: &str = "No mutants to filter";

/// Whether the run said it found no mutant in the scope it was given.
pub fn found_nothing(output: &str) -> bool {
    output.contains(NO_MUTANTS_FILTERED)
}

/// One pass's argv: the configured command, timed, reading the change diff
/// when there is one, carrying the run's output directory and the test
/// arguments of the suite it drives. A command that names its own output or,
/// under a scoped run, its own diff is a conflict mido refuses: it owns both.
pub fn pass_args(
    base: &[String],
    test_args: &[String],
    iterate: bool,
    output: &Path,
    in_diff: Option<&Path>,
) -> Result<Vec<String>, GuardrailsError> {
    if names_own_output(base) {
        return Err(GuardrailsError::setup(
            "the configured mutation command names its own --output",
        )
        .detail(
            "mido owns the mutation state: a verdict may only be decided by the \
             mutants a run itself started",
        )
        .hint(
            "drop --output from [mutation] command — mido writes into its own scratch directory",
        ));
    }
    if in_diff.is_some() && names_own_diff(base) {
        return Err(GuardrailsError::setup(
            "the configured mutation command names its own --in-diff",
        )
        .detail(
            "mido scopes a default mutation run to the change: it generates the \
             diff itself, so the two scopes would fight",
        )
        .hint(
            "drop --in-diff from [mutation] command, or set `in_diff = false` in \
             [mutation] to drive your own scope",
        ));
    }

    let mut args = base.to_vec();
    if iterate && !args.iter().any(|arg| arg == "--iterate") {
        args.push("--iterate".to_string());
    }
    args.push("--output".to_string());
    args.push(output.to_string_lossy().to_string());
    if let Some(diff) = in_diff {
        args.push("--in-diff".to_string());
        args.push(diff.to_string_lossy().to_string());
    }
    args.push("--timeout".to_string());
    args.push(TIMEOUT_SECS.to_string());
    if !test_args.is_empty() {
        args.push("--".to_string());
        args.extend(test_args.iter().cloned());
    }
    Ok(args)
}

/// Whether the configured command names its own output directory — in any of
/// the forms cargo-mutants' parser accepts.
fn names_own_output(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg.starts_with("-o") || arg.starts_with("--output"))
}

/// Whether the configured command names its own diff scope — cargo-mutants'
/// `--in-diff` and its `-D` short form.
fn names_own_diff(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg.starts_with("-D") || arg.starts_with("--in-diff"))
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

    #[test]
    fn a_pass_carries_the_output_directory_the_timeout_and_the_suite() {
        let args = pass_args(
            &cmd(&["cargo", "mutants"]),
            &cmd(&["--all-features"]),
            false,
            Path::new("/run/mutants"),
            None,
        )
        .expect("a pass");

        assert_eq!(
            args,
            cmd(&[
                "cargo",
                "mutants",
                "--output",
                "/run/mutants",
                "--timeout",
                "120",
                "--",
                "--all-features"
            ])
        );
    }

    #[test]
    fn a_scoped_pass_reads_the_change_diff() {
        let args = pass_args(
            &cmd(&["cargo", "mutants"]),
            &[],
            false,
            Path::new("/run"),
            Some(Path::new("/run/guardrails-mutants-workspace.diff")),
        )
        .expect("a pass");

        assert_eq!(
            args,
            cmd(&[
                "cargo",
                "mutants",
                "--output",
                "/run",
                "--in-diff",
                "/run/guardrails-mutants-workspace.diff",
                "--timeout",
                "120"
            ])
        );
    }

    #[test]
    fn an_iterated_pass_skips_what_the_earlier_passes_caught() {
        let args = pass_args(
            &cmd(&["cargo", "mutants"]),
            &[],
            true,
            Path::new("/run"),
            None,
        )
        .expect("a pass");

        assert!(args.iter().any(|arg| arg == "--iterate"));
    }

    #[test]
    fn a_command_that_names_its_own_output_is_refused() {
        let error = pass_args(
            &cmd(&["cargo", "mutants", "--output", "theirs"]),
            &[],
            false,
            Path::new("/run"),
            None,
        )
        .expect_err("a conflict");

        assert!(error.render().contains("names its own --output"));
    }

    #[test]
    fn a_command_that_names_its_own_diff_is_refused_when_the_run_scopes() {
        let base = cmd(&["cargo", "mutants", "--in-diff", "theirs.diff"]);

        let error = pass_args(
            &base,
            &[],
            false,
            Path::new("/run"),
            Some(Path::new("/run/mine.diff")),
        )
        .expect_err("a conflict");
        assert!(error.render().contains("names its own --in-diff"));

        assert!(
            pass_args(&base, &[], false, Path::new("/run"), None).is_ok(),
            "without a scope the command may carry its own diff"
        );
    }
}
