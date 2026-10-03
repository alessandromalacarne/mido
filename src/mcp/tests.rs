use super::*;
use std::cell::RefCell;

/// Stands in for a real mido invocation: cheap, deterministic, records argv.
struct Stub {
    result: Invocation,
    argv: RefCell<Vec<String>>,
}

impl Stub {
    fn new(code: i32) -> Self {
        Self {
            result: Invocation {
                code,
                stdout: format!("stub stdout {code}\n"),
                stderr: if code == 0 {
                    String::new()
                } else {
                    "stub stderr\n".to_string()
                },
            },
            argv: RefCell::new(Vec::new()),
        }
    }

    fn argv(&self) -> Vec<String> {
        self.argv.borrow().clone()
    }
}

impl Invoke for Stub {
    fn invoke(&self, argv: &[String]) -> Invocation {
        *self.argv.borrow_mut() = argv.to_vec();
        self.result.clone()
    }
}

fn request(line: &str) -> Value {
    handle_line(line, &Stub::new(0)).expect("a request is answered")
}

#[test]
fn notifications_are_not_answered() {
    let line = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

    assert!(handle_line(line, &Stub::new(0)).is_none());
}

#[test]
fn tools_list_exposes_the_ladder_and_target_discovery() {
    let response = request(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let tools = response["result"]["tools"]
        .as_array()
        .expect("a tools array")
        .clone();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();

    assert_eq!(names, ["list_targets", "run_ladder"]);
    for tool in &tools {
        assert!(tool["description"].is_string(), "{tool}");
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
    }

    let run = &tools[1]["inputSchema"]["properties"];
    assert_eq!(run["json"]["default"], true);
    assert_eq!(run["lang"]["enum"][0], "rust");
    assert_eq!(run["gates"]["items"]["enum"][5], "mutation");
    assert_eq!(run["target"]["default"], "auto");
    assert_eq!(run["repo"]["type"], "string");
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["repo"]["type"],
        "string"
    );
}

#[test]
fn run_ladder_maps_arguments_onto_the_cli() {
    let stub = Stub::new(1);
    let response = handle_line(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"run_ladder","arguments":{
            "target":"workspace","repo":"/repo","base":"origin/main",
            "paths":["src/a.rs","cli"],"gates":["tests","coverage"],
            "report":"out.md","json":false}}}"#,
        &stub,
    )
    .expect("answered");

    assert_eq!(
        stub.argv().join(" "),
        "workspace --repo /repo --base origin/main --path src/a.rs --path cli \
         --gate tests --gate coverage --report out.md"
    );
    assert_eq!(response["result"]["isError"], true);
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(text.contains("exit_code: 1"), "{text}");
    assert!(text.contains("stub stdout 1"), "{text}");
    assert!(text.contains("stub stderr"), "{text}");
}

#[test]
fn run_ladder_defaults_to_json_and_can_run_every_target() {
    let stub = Stub::new(0);
    let response = handle_line(
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"run_ladder","arguments":{"all":true,"apply_workspace_aid":true}}}"#,
        &stub,
    )
    .expect("answered");

    assert_eq!(stub.argv(), ["--all", "--apply-workspace-aid", "--json"]);
    assert_eq!(response["result"]["isError"], false);
}

#[test]
fn list_targets_maps_arguments_onto_the_cli() {
    let stub = Stub::new(0);
    handle_line(
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
            "name":"list_targets","arguments":{"repo":"/repo","lang":"rust"}}}"#,
        &stub,
    )
    .expect("answered");

    assert_eq!(
        stub.argv(),
        ["--list-targets", "--repo", "/repo", "--lang", "rust"]
    );
}

#[test]
fn a_malformed_line_is_a_parse_error() {
    let response = handle_line("{not json", &Stub::new(0)).expect("answered");

    assert!(response["id"].is_null());
    assert_eq!(response["error"]["code"], -32700);
}

#[test]
fn a_request_without_a_method_is_rejected() {
    let response = handle_line(r#"{"jsonrpc":"2.0","id":9}"#, &Stub::new(0)).expect("answered");

    assert_eq!(response["error"]["code"], -32600);
}

#[test]
fn an_unknown_method_is_rejected() {
    let line = r#"{"jsonrpc":"2.0","id":10,"method":"resources/list"}"#;
    let response = handle_line(line, &Stub::new(0)).expect("answered");

    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn an_unknown_tool_is_rejected() {
    let line = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call",
                   "params":{"name":"nope","arguments":{}}}"#;
    let response = handle_line(line, &Stub::new(0)).expect("answered");

    assert_eq!(response["error"]["code"], -32602);
    assert!(response["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("nope"));
}

#[test]
fn a_typo_in_an_argument_is_rejected_instead_of_ignored() {
    let line = r#"{"jsonrpc":"2.0","id":12,"method":"tools/call",
                   "params":{"name":"run_ladder","arguments":{"gate":["tests"]}}}"#;
    let response = handle_line(line, &Stub::new(0)).expect("answered");

    assert_eq!(response["error"]["code"], -32602);
    assert!(response["error"]["message"]
        .as_str()
        .expect("a message")
        .contains("gate"));
}

#[test]
fn ping_is_answered() {
    let response = handle_line(
        r#"{"jsonrpc":"2.0","id":13,"method":"ping"}"#,
        &Stub::new(0),
    )
    .expect("answered");

    assert!(response["result"].is_object());
}

#[test]
fn initialize_answers_the_handshake() {
    let response = request(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"test","version":"0"}}}"#,
    );

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(response["result"]["serverInfo"]["name"], "mido");
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert!(response["result"]["instructions"].is_string());
}

#[test]
fn a_message_that_is_not_an_object_is_rejected() {
    let response = handle_line("[1,2]", &Stub::new(0)).expect("answered");

    assert!(response["id"].is_null());
    assert_eq!(response["error"]["code"], -32600);
}

#[test]
fn tools_call_without_a_tool_name_is_rejected() {
    let line = r#"{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{}}"#;
    let response = handle_line(line, &Stub::new(0)).expect("answered");

    assert_eq!(response["error"]["code"], -32602);
}

#[test]
fn a_null_list_argument_is_treated_as_absent() {
    let stub = Stub::new(0);
    handle_line(
        r#"{"jsonrpc":"2.0","id":16,"method":"tools/call",
            "params":{"name":"run_ladder","arguments":{"paths":null,"gates":null}}}"#,
        &stub,
    )
    .expect("answered");

    assert_eq!(stub.argv(), ["--json"]);
}

#[test]
fn the_report_ends_cleanly_when_stdout_does_not() {
    let mut stub = Stub::new(0);
    stub.result.stdout = "no trailing newline".to_string();
    let response = handle_line(
        r#"{"jsonrpc":"2.0","id":15,"method":"tools/call",
            "params":{"name":"run_ladder","arguments":{}}}"#,
        &stub,
    )
    .expect("answered");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");

    assert!(text.ends_with('\n'), "{text:?}");
    assert!(!text.contains("--- stderr ---"), "{text:?}");
}

#[test]
fn the_stdio_loop_answers_requests_and_skips_notifications() {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n",
        "\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
    );
    let mut output = Vec::new();

    serve(std::io::Cursor::new(input), &mut output, &Stub::new(0)).expect("the loop completes");

    let lines: Vec<&str> = std::str::from_utf8(&output)
        .expect("utf8")
        .lines()
        .collect();
    assert_eq!(lines.len(), 2, "{output:?}");
    let first: Value = serde_json::from_str(lines[0]).expect("json");
    let second: Value = serde_json::from_str(lines[1]).expect("json");
    assert_eq!(first["id"], 1);
    assert_eq!(second["id"], 2);
}

#[test]
fn the_system_invoker_runs_the_cli_in_process() {
    let repo = workspace_repo();
    let root = repo.path().to_str().expect("utf8").to_string();

    let outcome = SystemInvoke.invoke(&["--list-targets".to_string(), "--repo".to_string(), root]);

    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert!(outcome.stdout.contains("workspace"), "{}", outcome.stdout);
}

#[test]
fn the_system_invoker_reports_a_usage_error_like_the_cli() {
    let outcome = SystemInvoke.invoke(&[
        "--path".to_string(),
        "src".to_string(),
        "--base".to_string(),
        "origin/main".to_string(),
    ]);

    assert_eq!(outcome.code, 2, "{}", outcome.stderr);
    assert!(!outcome.stderr.is_empty());
}

fn workspace_repo() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.0\"\n",
    )
    .expect("manifest written");
    directory
}
