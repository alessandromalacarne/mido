//! `.guardrails.toml` is the tool contract.

pub mod keys;
pub mod suggest;
pub mod text;
pub mod validate;
pub mod value;

use crate::error::GuardrailsError;
use crate::targets::Target;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threshold {
    pub warn: i64,
    pub fail: i64,
}

impl Threshold {
    pub fn new(warn: i64, fail: i64) -> Self {
        Self { warn, fail }
    }

    fn from_value(value: &Value, default: Threshold) -> Threshold {
        let Some(table) = value.as_table() else {
            return default;
        };
        Threshold {
            warn: table
                .get("warn")
                .and_then(value::as_int)
                .unwrap_or(default.warn),
            fail: table
                .get("fail")
                .and_then(value::as_int)
                .unwrap_or(default.fail),
        }
    }
}

/// The parsed tool contract, plus the warnings it produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    path: Option<PathBuf>,
    data: Table,
    warnings: Vec<String>,
}

impl Config {
    pub fn load(repo: &Path) -> Result<Self, GuardrailsError> {
        let path = repo.join(".guardrails.toml");
        if !path.exists() {
            return Ok(Self {
                path: None,
                data: Table::new(),
                warnings: vec![format!(
                    "no {} — falling back to built-in default thresholds",
                    path.display()
                )],
            });
        }

        let text = std::fs::read_to_string(&path).map_err(|error| {
            GuardrailsError::config(format!("{} is not readable", path.display()))
                .detail(error.to_string())
        })?;
        let data = text.parse::<Table>().map_err(|error| {
            GuardrailsError::config(format!("{} is not parseable toml", path.display()))
                .detail(error.to_string())
                .hint("the ladder refuses to guess thresholds from a broken config")
        })?;

        validate::validate_config(&data, &text, &path)?;
        validate::validate_script_entry(&data, &text, &path, repo)?;

        let warnings = if data.contains_key("version") {
            Vec::new()
        } else {
            vec![format!(
                "{} has no `version = 1` — the guardrails schema requires it",
                path.display()
            )]
        };
        Ok(Self {
            path: Some(path),
            data,
            warnings,
        })
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn data(&self) -> &Table {
        &self.data
    }

    pub fn source(&self) -> String {
        match &self.path {
            Some(path) => format!(
                "`{}`",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            None => "built-in defaults".to_string(),
        }
    }

    /// The runner `.guardrails.toml` declares, if it declares one.
    pub fn script(&self) -> Option<String> {
        self.data
            .get("script")
            .and_then(value::as_str)
            .map(str::to_string)
    }

    pub fn target_table(&self, name: &str) -> Option<&Table> {
        self.data
            .get("targets")
            .and_then(value::as_table)
            .and_then(|targets| targets.get(name))
            .and_then(value::as_table)
    }

    /// Root `[gate]` merged with `[targets.<target>.<gate>]`; target wins.
    pub fn section(&self, gate: &str, target: Option<&str>) -> Table {
        let mut merged = self
            .data
            .get(gate)
            .and_then(value::as_table)
            .cloned()
            .unwrap_or_default();

        if let Some(name) = target.filter(|name| *name != "workspace") {
            if let Some(overrides) = self
                .target_table(name)
                .and_then(|table| table.get(gate))
                .and_then(value::as_table)
            {
                merged.extend(overrides.clone());
            }
        }
        merged
    }

    fn stored(&self, gate: &str, key: &str, target: Option<&str>) -> Option<&Value> {
        if let Some(name) = target.filter(|name| *name != "workspace") {
            if let Some(value) = self
                .target_table(name)
                .and_then(|table| table.get(gate))
                .and_then(value::as_table)
                .and_then(|section| section.get(key))
            {
                return Some(value);
            }
        }
        self.data
            .get(gate)
            .and_then(value::as_table)
            .and_then(|section| section.get(key))
    }

    pub fn float(&self, gate: &str, key: &str, default: f64, target: Option<&str>) -> f64 {
        self.stored(gate, key, target)
            .and_then(value::as_float)
            .unwrap_or(default)
    }

    pub fn int(&self, gate: &str, key: &str, default: i64, target: Option<&str>) -> i64 {
        self.stored(gate, key, target)
            .and_then(value::as_int)
            .unwrap_or(default)
    }

    pub fn text_setting(
        &self,
        gate: &str,
        key: &str,
        default: &str,
        target: Option<&str>,
    ) -> String {
        self.stored(gate, key, target)
            .and_then(value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default.to_string())
    }

    pub fn threshold(
        &self,
        gate: &str,
        key: &str,
        default: Threshold,
        target: Option<&str>,
    ) -> Threshold {
        match self.stored(gate, key, target) {
            Some(value) => Threshold::from_value(value, default),
            None => default,
        }
    }

    pub fn enabled(&self, gate: &str, target: Option<&str>) -> bool {
        !matches!(
            self.stored(gate, "enabled", target),
            Some(Value::Boolean(false))
        )
    }

    /// The command this gate runs for this target, and where it came from.
    ///
    /// `[targets.<name>.<gate>]` wins. A root section is written for the
    /// repo-level gate, so the workspace and its members inherit it — a
    /// standalone crate (something the root manifest excludes) does not:
    /// `--all-features` written for the workspace root pulls the backend
    /// features into a wasm crate and breaks the build.
    pub fn command(&self, gate: &str, key: &str, target: &Target, default: &str) -> String {
        if let Some(value) = self
            .target_table(&target.name)
            .and_then(|table| table.get(gate))
            .and_then(value::as_table)
            .and_then(|section| section.get(key))
        {
            return value::render(value);
        }
        if target.workspace_member {
            if let Some(value) = self
                .data
                .get(gate)
                .and_then(value::as_table)
                .and_then(|section| section.get(key))
            {
                return value::render(value);
            }
        }
        default.to_string()
    }

    /// The commands a gate runs, written either as one string or as a list.
    pub fn commands(
        &self,
        gate: &str,
        key: &str,
        target: &Target,
        default: Vec<String>,
    ) -> Vec<String> {
        if let Some(value) = self
            .target_table(&target.name)
            .and_then(|table| table.get(gate))
            .and_then(value::as_table)
            .and_then(|section| section.get(key))
        {
            if let Some(commands) = value::command_list(value) {
                return commands;
            }
        }
        if target.workspace_member {
            if let Some(value) = self
                .data
                .get(gate)
                .and_then(value::as_table)
                .and_then(|section| section.get(key))
            {
                if let Some(commands) = value::command_list(value) {
                    return commands;
                }
            }
        }
        default
    }
}

#[cfg(test)]
mod tests;
