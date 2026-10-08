//! The tools this server exposes: their schemas and the argv they map to.

use serde_json::{json, Value};

/// The tool catalog, as `tools/list` answers it.
pub(super) fn schemas() -> Vec<Value> {
    vec![list_targets_schema(), run_ladder_schema()]
}

fn list_targets_schema() -> Value {
    json!({
        "name": "list_targets",
        "description": "List every target the ladder can measure in the repository — the \
                        workspace, its members, standalone crates and declared targets — with \
                        name, path and kind. Call it before naming a package in run_ladder.",
        "inputSchema": {
            "type": "object",
            "properties": { "repo": repo_arg(), "lang": lang_arg() },
            "additionalProperties": false,
        },
    })
}

fn run_ladder_schema() -> Value {
    json!({
        "name": "run_ladder",
        "description": "Run the guardrails ladder (`.mido.toml`) against the whole workspace, \
                        or the packages `-p` names, and return its report: exit_code 0 \
                        SHIP-READY, 1 BLOCKED (a gate failed; the report carries evidence and \
                        fix hints), 2 INCOMPLETE (a gate could not run) or a setup error. Tests \
                        and mutation can run for many minutes.",
        "inputSchema": {
            "type": "object",
            "properties": run_ladder_properties(),
            "additionalProperties": false,
        },
    })
}

fn run_ladder_properties() -> Value {
    let mut properties = selection_properties();
    merge(&mut properties, ladder_switches());
    merge(&mut properties, reporting_properties());
    properties
}

/// What the run measures: the repo and the packages.
fn selection_properties() -> Value {
    json!({
        "repo": repo_arg(),
        "lang": lang_arg(),
        "packages": {
            "type": "array",
            "items": { "type": "string" },
            "description": "measure only these packages, by cargo package name; default: \
                            the whole workspace",
        },
    })
}

/// Which gates run.
fn ladder_switches() -> Value {
    let gate_names: Vec<&str> = crate::gate::GATES.iter().map(|gate| gate.name()).collect();
    json!({
        "gates": {
            "type": "array",
            "items": {
                "type": "string",
                "enum": gate_names,
            },
            "description": "run only these gates; default: the whole ladder",
        },
        "apply_workspace_aid": {
            "type": "boolean",
            "default": false,
            "description": "add the local [workspace] aid when a worktree's manifest misses \
                            one, so cargo resolves the crate's own root",
        },
    })
}

/// Where the evidence goes.
fn reporting_properties() -> Value {
    json!({
        "baseline_lcov": {
            "type": "string",
            "description": "lcov from the base revision, for the coverage delta",
        },
        "report": {
            "type": "string",
            "description": "write the markdown report to this path",
        },
        "json": {
            "type": "boolean",
            "default": true,
            "description": "append the machine-readable verdict JSON to the output",
        },
    })
}

/// Fold one json object's entries into another.
fn merge(target: &mut Value, extra: Value) {
    let object = target.as_object_mut().expect("json object");
    for (key, value) in extra.as_object().expect("json object") {
        object.insert(key.clone(), value.clone());
    }
}

fn repo_arg() -> Value {
    json!({
        "type": "string",
        "description": "repository (or worktree) root; defaults to this server's working directory",
    })
}

fn lang_arg() -> Value {
    let names: Vec<&str> = crate::lang::LANGS.iter().map(|lang| lang.name()).collect();
    json!({
        "type": "string",
        "enum": names,
        "description": "language module; default: inferred from the repo",
    })
}

/// The mido command line a tool call stands for.
pub(super) fn argv_for(name: &str, arguments: &Value) -> Result<Vec<String>, String> {
    if !arguments.is_object() {
        return Err("arguments must be an object".to_string());
    }

    match name {
        "list_targets" => list_targets(arguments),
        "run_ladder" => run_ladder(arguments),
        _ => Err(format!("unknown tool `{name}`")),
    }
}

fn list_targets(arguments: &Value) -> Result<Vec<String>, String> {
    reject_unknown(arguments, "list_targets", &["repo", "lang"])?;

    let mut argv = vec!["--list-targets".to_string()];
    value_flags(
        &mut argv,
        arguments,
        &[("--repo", "repo"), ("--lang", "lang")],
    )?;
    Ok(argv)
}

/// Every argument `run_ladder` accepts; anything else is a typo.
const RUN_LADDER_ARGUMENTS: [&str; 8] = [
    "repo",
    "lang",
    "packages",
    "gates",
    "apply_workspace_aid",
    "baseline_lcov",
    "report",
    "json",
];

fn run_ladder(arguments: &Value) -> Result<Vec<String>, String> {
    reject_unknown(arguments, "run_ladder", &RUN_LADDER_ARGUMENTS)?;

    let mut argv = Vec::new();
    value_flags(
        &mut argv,
        arguments,
        &[("--repo", "repo"), ("--lang", "lang")],
    )?;
    list_flags(
        &mut argv,
        arguments,
        &[("-p", "packages"), ("--gate", "gates")],
    )?;
    switch_flags(
        &mut argv,
        arguments,
        &[
            ("--apply-workspace-aid", "apply_workspace_aid", false),
            ("--json", "json", true),
        ],
    )?;
    value_flags(
        &mut argv,
        arguments,
        &[("--baseline-lcov", "baseline_lcov"), ("--report", "report")],
    )?;

    Ok(argv)
}

/// The one-value flags: `--flag value` when the argument is present.
fn value_flags(
    argv: &mut Vec<String>,
    arguments: &Value,
    flags: &[(&str, &str)],
) -> Result<(), String> {
    for (flag, key) in flags {
        flag_optional(argv, flag, string_arg(arguments, key)?);
    }
    Ok(())
}

/// The repeatable flags: one `--flag value` per entry.
fn list_flags(
    argv: &mut Vec<String>,
    arguments: &Value,
    flags: &[(&str, &str)],
) -> Result<(), String> {
    for (flag, key) in flags {
        for value in string_list_arg(arguments, key)? {
            argv.push(flag.to_string());
            argv.push(value);
        }
    }
    Ok(())
}

/// The on/off flags, pushed only when on.
fn switch_flags(
    argv: &mut Vec<String>,
    arguments: &Value,
    flags: &[(&str, &str, bool)],
) -> Result<(), String> {
    for (flag, key, default) in flags {
        if bool_arg(arguments, key, *default)? {
            argv.push(flag.to_string());
        }
    }
    Ok(())
}

/// An argument this server does not know is a mistake, never a silent no-op.
fn reject_unknown(arguments: &Value, tool: &str, known: &[&str]) -> Result<(), String> {
    for key in arguments
        .as_object()
        .into_iter()
        .flat_map(|arguments| arguments.keys())
    {
        if !known.contains(&key.as_str()) {
            return Err(format!(
                "{tool}: unknown argument `{key}` (known: {})",
                known.join(", ")
            ));
        }
    }
    Ok(())
}

fn string_arg(arguments: &Value, key: &str) -> Result<Option<String>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("`{key}` must be a string")),
    }
}

fn string_list_arg(arguments: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(Vec::new());
    };
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(value) => Ok(value.clone()),
                _ => Err(format!("`{key}` must be a list of strings")),
            })
            .collect(),
        _ => Err(format!("`{key}` must be a list of strings")),
    }
}

fn bool_arg(arguments: &Value, key: &str, default: bool) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("`{key}` must be a boolean")),
    }
}

fn flag_optional(argv: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        argv.push(flag.to_string());
        argv.push(value);
    }
}
