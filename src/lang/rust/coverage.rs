//! coverage: where cargo-llvm-cov's argv says the lcov report lands.

use std::path::PathBuf;

/// The path `--output-path <path>` or `--output-path=<path>` names, if any.
pub fn report_path(argv: &[String]) -> Option<PathBuf> {
    let mut args = argv.iter();
    while let Some(arg) = args.next() {
        if arg == "--output-path" {
            return args.next().map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--output-path=") {
            return Some(PathBuf::from(value));
        }
    }
    None
}
