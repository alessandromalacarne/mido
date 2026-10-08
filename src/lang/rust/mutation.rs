//! cargo-mutants: its report files, and the test arguments a command carries.

pub mod report;

pub const TOOL: &str = "cargo-mutants";
pub const TIMEOUT_SECS: i64 = 120;

/// The arguments a declared test command carries to `cargo test`: everything
/// after the `cargo test` it names. A command that names none carries nothing —
/// the mutation tool's own default test command then applies.
pub fn test_args(command: &[String]) -> Option<Vec<String>> {
    let start = command
        .windows(2)
        .position(|pair| pair[0] == "cargo" && pair[1] == "test")?;

    Some(command[start + 2..].to_vec())
}

