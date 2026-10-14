//! cargo-mutants: its pass argv, its report files, and the test arguments a
//! command carries.

pub mod report;

use crate::error::GuardrailsError;
use std::path::Path;

pub const TOOL: &str = "cargo-mutants";
pub const TIMEOUT_SECS: i64 = 120;

/// One pass's argv: the configured command, timed, carrying the run's output
/// directory and the test arguments of the suite it drives.
pub fn pass_args(
    base: &[String],
    test_args: &[String],
    iterate: bool,
    output: &Path,
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

    let mut args = base.to_vec();
    if iterate && !args.iter().any(|arg| arg == "--iterate") {
        args.push("--iterate".to_string());
    }
    args.push("--output".to_string());
    args.push(output.to_string_lossy().to_string());
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
    fn a_pass_carries_the_output_directory_the_timeout_and_the_suite() {
        let args = pass_args(
            &cmd(&["cargo", "mutants"]),
            &cmd(&["--all-features"]),
            false,
            Path::new("/run/mutants"),
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
    fn an_iterated_pass_skips_what_the_earlier_passes_caught() {
        let args =
            pass_args(&cmd(&["cargo", "mutants"]), &[], true, Path::new("/run")).expect("a pass");

        assert!(args.iter().any(|arg| arg == "--iterate"));
    }

    #[test]
    fn a_command_that_names_its_own_output_is_refused() {
        let error = pass_args(
            &cmd(&["cargo", "mutants", "--output", "theirs"]),
            &[],
            false,
            Path::new("/run"),
        )
        .expect_err("a conflict");

        assert!(error.render().contains("names its own --output"));
    }
}
