use super::keys::{self, GATES};
use super::suggest::suggestion;
use super::text::{is_section, located};
use super::value::type_name;
use crate::error::GuardrailsError;
use std::path::Path;
use toml::{Table, Value};

pub fn validate_config(
    data: &Table,
    text: &str,
    config_path: &Path,
) -> Result<(), GuardrailsError> {
    for (key, value) in data {
        if !keys::top_level_keys().contains(key.as_str()) {
            return Err(reject_unknown_top_level_key(key, text, config_path));
        }
        if GATES.contains(&key.as_str()) || key == "failure" {
            validate_gate_section(text, config_path, key, value, &format!("[{key}]"))?;
        } else if key == "targets" {
            validate_targets_table(value, text, config_path)?;
        }
    }
    Ok(())
}

pub fn reject_unknown_top_level_key(key: &str, text: &str, config_path: &Path) -> GuardrailsError {
    let kind = if is_section(text, key) {
        format!("section `[{key}]`")
    } else {
        format!("key `{key}` at the top level")
    };
    let known = keys::top_level_keys();

    GuardrailsError::config(format!("{} is not valid", config_path.display()))
        .detail(format!("{}unknown {kind}", located(text, key, None)))
        .detail(format!(
            "top level accepts: {} — {}",
            keys::joined(known.iter().copied()),
            suggestion(key, &known)
        ))
        .hint("the ladder refuses to run against a config it cannot read as written, because a typo would move a threshold")
}

pub fn unknown_key_error(
    config_path: &Path,
    text: &str,
    key: &str,
    section: &str,
    known: &std::collections::BTreeSet<&'static str>,
) -> GuardrailsError {
    let where_ = format!("[{section}]");

    GuardrailsError::config(format!("{} is not valid", config_path.display()))
        .detail(format!(
            "{}unknown key `{key}` in {where_}",
            located(text, key, Some(section))
        ))
        .detail(format!(
            "{where_} accepts: {} — {}",
            keys::joined(known.iter().copied()),
            suggestion(key, known)
        ))
        .hint("an unknown key is a config error on purpose, so a typo cannot silently disable a gate or move a threshold")
}

pub fn validate_targets_table(
    value: &Value,
    text: &str,
    config_path: &Path,
) -> Result<(), GuardrailsError> {
    let Some(targets) = value.as_table() else {
        return Err(
            GuardrailsError::config(format!("{} is not valid", config_path.display()))
                .detail("`targets` must hold one table per target, e.g. [targets.frontend]"),
        );
    };

    for (name, section) in targets {
        validate_target_section(text, config_path, name, section)?;
    }
    Ok(())
}

pub fn validate_target_section(
    text: &str,
    config_path: &Path,
    name: &str,
    section: &Value,
) -> Result<(), GuardrailsError> {
    let Some(entries) = section.as_table() else {
        return Err(
            GuardrailsError::config(format!("{} is not valid", config_path.display()))
                .detail(format!("`[targets.{name}]` must be a table")),
        );
    };

    let known = keys::target_keys();
    for (key, value) in entries {
        if !known.contains(key.as_str()) {
            return Err(unknown_key_error(
                config_path,
                text,
                key,
                &format!("targets.{name}"),
                &known,
            ));
        }
        if GATES.contains(&key.as_str()) {
            validate_gate_section(
                text,
                config_path,
                key,
                value,
                &format!("[targets.{name}.{key}]"),
            )?;
        }
    }
    Ok(())
}

pub fn validate_gate_section(
    text: &str,
    config_path: &Path,
    gate: &str,
    section: &Value,
    label: &str,
) -> Result<(), GuardrailsError> {
    let Some(entries) = section.as_table() else {
        return Err(
            GuardrailsError::config(format!("{} is not valid", config_path.display()))
                .detail(format!("{label} must be a table")),
        );
    };

    let section_name = label.trim_matches(['[', ']']);
    let known = keys::section_keys(gate);
    for (key, value) in entries {
        if !known.contains(key.as_str()) {
            return Err(unknown_key_error(
                config_path,
                text,
                key,
                section_name,
                &known,
            ));
        }
        validate_threshold_table(config_path, section_name, key, value)?;
    }
    Ok(())
}

fn validate_threshold_table(
    config_path: &Path,
    section_name: &str,
    key: &str,
    value: &Value,
) -> Result<(), GuardrailsError> {
    let Some(nested) = value.as_table() else {
        return Ok(());
    };
    for nested_key in nested.keys() {
        if !keys::THRESHOLD_KEYS.contains(&nested_key.as_str()) {
            return Err(
                GuardrailsError::config(format!("{} is not valid", config_path.display()))
                    .detail(format!(
                        "unknown key `{nested_key}` inside `{section_name} {key}`"
                    ))
                    .detail(format!(
                        "a `{key}` threshold takes only: {}",
                        keys::joined(keys::THRESHOLD_KEYS)
                    ))
                    .hint("write it as `key = { warn = 300, fail = 500 }`"),
            );
        }
    }
    Ok(())
}

/// `script` names the runner that implements this ladder.
///
/// Declared rather than inferred: a value that is not a string, or that points at
/// a file the repo does not have, is a config error.
pub fn validate_script_entry(
    data: &Table,
    text: &str,
    config_path: &Path,
    repo: &Path,
) -> Result<(), GuardrailsError> {
    let Some(value) = data.get("script") else {
        return Ok(());
    };
    let at = located(text, "script", None);

    let Value::String(declared) = value else {
        return Err(
            GuardrailsError::config(format!("{} is not valid", config_path.display()))
                .detail(format!(
                    "{at}`script` must be a repo-relative path, got {}",
                    type_name(value)
                ))
                .hint(r#"write it as script = "scripts/guardrails.py""#),
        );
    };

    if !repo.join(declared).exists() {
        return Err(GuardrailsError::config(format!("{} is not valid", config_path.display()))
            .detail(format!(
                "{at}`script` points at `{declared}`, and there is no such file under {}",
                repo.display()
            ))
            .hint("a declared runner that is not there is worse than no declaration — fix the path or drop the key"));
    }
    Ok(())
}
