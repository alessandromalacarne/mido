//! `.mido.toml` is the tool contract.

pub mod keys;
pub mod text;
pub mod validate;
pub mod value;

use crate::error::GuardrailsError;
use crate::lang::Lang;
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
    defaults: Table,
    lang_name: &'static str,
    warnings: Vec<String>,
}

impl Config {
    pub fn load(repo: &Path, lang: &Lang) -> Result<Self, GuardrailsError> {
        let defaults = lang.default_config().clone();
        let lang_name = lang.name();
        let path = repo.join(".mido.toml");
        if !path.exists() {
            return Ok(Self {
                path: None,
                data: Table::new(),
                defaults,
                lang_name,
                warnings: vec![format!(
                    "no {} — falling back to the {lang_name} built-in defaults",
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
            defaults,
            lang_name,
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
            None => format!("{} built-in defaults", self.lang_name),
        }
    }

    pub fn target_table(&self, name: &str) -> Option<&Table> {
        self.data
            .get("targets")
            .and_then(value::as_table)
            .and_then(|targets| targets.get(name))
            .and_then(value::as_table)
    }

    /// Root `[gate]` merged with `[targets.<target>.<gate>]` — over the module
    /// defaults, so the merged view is what actually applies; target wins.
    pub fn section(&self, gate: &str, target: Option<&str>) -> Table {
        let mut merged = self
            .defaults
            .get(gate)
            .and_then(value::as_table)
            .cloned()
            .unwrap_or_default();

        if let Some(root) = self.data.get(gate).and_then(value::as_table) {
            merged.extend(root.clone());
        }

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

    /// The file's own `[gate]`/`[targets.<name>.<gate>]` value, with the module
    /// defaults as the last layer.
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
            .or_else(|| {
                self.defaults
                    .get(gate)
                    .and_then(value::as_table)
                    .and_then(|section| section.get(key))
            })
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

    /// The argv this gate runs for this target, when the config declares one.
    ///
    /// `[targets.<name>.<gate>]` wins. A root section — the file's or the
    /// module defaults' — is written for the repo-level gate, so the workspace
    /// and its members inherit it; a standalone crate (something the root
    /// manifest excludes) does not: `--all-features` written for the workspace
    /// root pulls the backend features into a wasm crate and breaks the build.
    /// Standalone crates fall back to the module's bare commands instead.
    pub fn argv(&self, gate: &str, key: &str, target: &Target) -> Option<Vec<String>> {
        let overridden = self
            .target_table(&target.name)
            .and_then(|table| table.get(gate))
            .and_then(value::as_table)
            .and_then(|section| section.get(key));
        if let Some(value) = overridden {
            return value::string_array(value);
        }
        if target.workspace_member {
            let inherited = self
                .data
                .get(gate)
                .and_then(value::as_table)
                .and_then(|section| section.get(key));
            if let Some(value) = inherited {
                return value::string_array(value);
            }
            let baseline = self
                .defaults
                .get(gate)
                .and_then(value::as_table)
                .and_then(|section| section.get(key));
            if let Some(value) = baseline {
                return value::string_array(value);
            }
        }
        None
    }

    /// The `[failure] max_attempts_per_gate` the report cites, from the file or
    /// the module defaults.
    pub fn attempts_cap(&self) -> Option<i64> {
        self.stored("failure", "max_attempts_per_gate", None)
            .and_then(value::as_int)
    }
}

#[cfg(test)]
mod tests;
