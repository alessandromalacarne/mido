//! MCP server: the ladder as tools an agent calls.
//!
//! JSON-RPC 2.0, one message per line over stdio.

use crate::cli;
use crate::process::SystemRunner;
use crate::style::Style;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

mod tools;

/// The protocol revisions this server speaks, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What the client is told about using this server.
const INSTRUCTIONS: &str = "\
mido runs the six-gate guardrails ladder (syntax, size, analysis, tests, coverage, mutation) \
against one target of a repository. Call run_ladder to measure a change: the text result is the \
report meant for you. exit_code 0 = SHIP-READY, 1 = BLOCKED (a gate failed; the fix hints are \
inside), 2 = INCOMPLETE (a gate could not run — usually missing tooling) or a setup error. \
list_targets shows what can be measured before naming a target.";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// The terminal outcome of one mido invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a mido command line and captures its streams.
pub trait Invoke {
    fn invoke(&self, argv: &[String]) -> Invocation;
}

/// The real invoker: the exact CLI `mido` itself parses and runs.
#[derive(Debug, Clone, Copy)]
pub struct SystemInvoke;

impl Invoke for SystemInvoke {
    fn invoke(&self, argv: &[String]) -> Invocation {
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();

        match cli::try_parse_from(&argv) {
            Ok(args) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                let code = cli::main_with(
                    &args,
                    &SystemRunner,
                    &mut stdout,
                    &mut stderr,
                    Style::detect(),
                );
                Invocation {
                    code,
                    stdout: String::from_utf8_lossy(&stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr).into_owned(),
                }
            }
            Err(error) => Invocation {
                code: error.exit_code(),
                stdout: String::new(),
                stderr: error.render().to_string(),
            },
        }
    }
}

/// Serve MCP on this process's stdio until the client closes it.
pub fn serve_stdio() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    match serve(stdin.lock(), &mut stdout.lock(), &SystemInvoke) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("mido mcp: {error}");
            1
        }
    }
}

/// One JSON-RPC message per line in, one per line out.
fn serve(input: impl BufRead, output: &mut dyn Write, invoke: &dyn Invoke) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Some(response) = handle_line(&line, invoke) else {
            continue;
        };
        serde_json::to_writer(&mut *output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

/// Answer one JSON-RPC line; `None` when the message asks for no answer.
pub fn handle_line(line: &str, invoke: &dyn Invoke) -> Option<Value> {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Some(error_response(
            Value::Null,
            PARSE_ERROR,
            "the line is not JSON",
        ));
    };
    let Some(object) = message.as_object() else {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "a message must be a JSON object",
        ));
    };

    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return object
            .get("id")
            .cloned()
            .map(|id| error_response(id, INVALID_REQUEST, "a request needs a method"));
    };
    let id = object.get("id").cloned()?;

    match method {
        "initialize" => Some(success(id, initialize_result(message.get("params")))),
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, json!({ "tools": tools::schemas() }))),
        "tools/call" => Some(call_tool(id, message.get("params"), invoke)),
        _ => Some(error_response(
            id,
            METHOD_NOT_FOUND,
            &format!("unknown method `{method}`"),
        )),
    }
}

/// Answer one `tools/call`: build the mido argv, run it, wrap the report.
fn call_tool(id: Value, params: Option<&Value>, invoke: &dyn Invoke) -> Value {
    let Some(name) = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
    else {
        return error_response(id, INVALID_PARAMS, "tools/call needs a tool name");
    };
    let empty = json!({});
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .unwrap_or(&empty);

    match tools::argv_for(name, arguments) {
        Ok(argv) => success(id, tool_result(&invoke.invoke(&argv))),
        Err(reason) => error_response(id, INVALID_PARAMS, &reason),
    }
}

/// The report a call returns: the exit code first, then what the run printed.
fn tool_result(outcome: &Invocation) -> Value {
    let mut text = format!("exit_code: {}\n\n{}", outcome.code, outcome.stdout);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    if !outcome.stderr.is_empty() {
        text.push_str("--- stderr ---\n");
        text.push_str(&outcome.stderr);
    }

    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": outcome.code != 0,
    })
}

/// The handshake: the client's revision when this server speaks it, else the newest.
fn initialize_result(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|version| **version == requested)
        .copied()
        .unwrap_or(PROTOCOL_VERSIONS[0]);

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "mido", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

#[cfg(test)]
mod tests;
