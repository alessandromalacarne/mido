//! stryker: the config mido drives it with, its pass argv, and its report.

pub mod report;

use crate::error::GuardrailsError;
use std::path::{Path, PathBuf};

pub const TOOL: &str = "stryker";

/// The config mido writes into the run's own output directory, and passes to
/// stryker as its positional config argument.
pub const CONFIG_FILE: &str = "stryker-mido.config.json";

/// The config files a repo may carry, in stryker's own discovery order.
pub const REPO_CONFIGS: [&str; 5] = [
    "stryker.config.json",
    "stryker.config.js",
    "stryker.config.mjs",
    "stryker.config.cjs",
    "stryker.config.ts",
];

/// The arguments a declared test command carries to stryker: none — stryker
/// runs the test runner its own config names.
pub fn test_args(_command: &[String]) -> Option<Vec<String>> {
    None
}

/// The config file the repo itself carries, if any.
pub fn repo_config(work_dir: &Path) -> Option<PathBuf> {
    REPO_CONFIGS
        .iter()
        .map(|name| work_dir.join(name))
        .find(|path| path.exists())
}

/// Write the config mido drives stryker with into the run's own output
/// directory: the repo's own config extended (when it carries one), the json
/// reporter pointed at the run's own report path. The gate empties the output
/// directory before every run, so the report the verdict reads can only come
/// from the mutants this run tested.
pub fn prepare(output: &Path, work_dir: &Path) -> Result<(), GuardrailsError> {
    let mut config = serde_json::Map::new();
    if let Some(repo) = repo_config(work_dir) {
        config.insert(
            "extends".to_string(),
            serde_json::Value::String(repo.to_string_lossy().to_string()),
        );
    }
    config.insert("reporters".to_string(), serde_json::json!(["json"]));
    config.insert(
        "jsonReporter".to_string(),
        serde_json::json!({ "fileName": report::report_path(output).to_string_lossy() }),
    );

    std::fs::create_dir_all(output).map_err(|error| {
        GuardrailsError::setup(format!("cannot create {}", output.display()))
            .detail(error.to_string())
    })?;
    std::fs::write(
        output.join(CONFIG_FILE),
        serde_json::to_string_pretty(&config).unwrap_or_default(),
    )
    .map_err(|error| {
        GuardrailsError::setup(format!(
            "cannot write {}",
            output.join(CONFIG_FILE).display()
        ))
        .detail(error.to_string())
    })
}

/// One pass's argv: the configured command, carrying mido's own config. The
/// test arguments a pass could carry are none — stryker runs the test runner
/// its config names. A command that names a config of its own is a conflict,
/// not an override.
pub fn pass_args(
    base: &[String],
    _test_args: &[String],
    _iterate: bool,
    output: &Path,
) -> Result<Vec<String>, GuardrailsError> {
    if names_own_config(base) {
        return Err(GuardrailsError::setup(
            "the configured mutation command names its own stryker config",
        )
        .detail(
            "mido owns the mutation state: the report path must stay inside the \
             run's own output directory",
        )
        .hint(
            "drop the config path from [mutation] command — mido extends the repo's \
             stryker config itself",
        ));
    }

    let mut args = base.to_vec();
    args.push(output.join(CONFIG_FILE).to_string_lossy().to_string());
    Ok(args)
}

/// Whether the configured command carries a stryker config of its own.
fn names_own_config(args: &[String]) -> bool {
    args.iter()
        .any(|arg| REPO_CONFIGS.iter().any(|name| arg.ends_with(name)))
}

#[cfg(test)]
mod tests;
