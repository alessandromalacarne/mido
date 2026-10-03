//! The `mido mcp` subcommand: MCP over stdio, driving the real engine.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mido"))
            .arg("mcp")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").expect("a line is written");
        self.stdin.flush().expect("flushed");
    }

    fn request(&mut self, line: &str) -> Value {
        self.send(line);
        let mut response = String::new();
        self.stdout.read_line(&mut response).expect("an answer");
        serde_json::from_str(&response).expect("the answer is JSON")
    }

    fn call(&mut self, id: i64, name: &str, arguments: Value) -> Value {
        let line = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        })
        .to_string();

        self.request(&line)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write(path: PathBuf, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, body).expect("file written");
}

/// A miniature workspace repo: the engine must find `workspace` and `lib`.
fn mini_repo() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("temp dir");
    write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"lib\"]\n",
    );
    write(
        directory.path().join("lib/Cargo.toml"),
        "[package]\nname = \"lib\"\n",
    );
    directory
}

fn text(response: &Value) -> &str {
    response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content")
}

#[test]
fn the_subcommand_speaks_mcp_over_stdio() {
    let repo = mini_repo();
    let mut server = Server::start(repo.path());

    let handshake = server.request(
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "t", "version": "0" },
            },
        })
        .to_string(),
    );

    assert_eq!(handshake["result"]["serverInfo"]["name"], "mido");
    assert_eq!(handshake["result"]["protocolVersion"], "2025-06-18");
    server.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let tools = server.request(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();

    assert_eq!(names, ["list_targets", "run_ladder"]);
}

#[test]
fn list_targets_runs_the_real_engine() {
    let repo = mini_repo();
    let root = repo.path().to_str().expect("utf8").to_string();
    let mut server = Server::start(repo.path());

    let response = server.call(3, "list_targets", json!({ "repo": root }));
    let result = &response["result"];

    assert_eq!(result["isError"], false);
    assert!(text(&response).contains("workspace"), "{}", text(&response));
    assert!(text(&response).contains("lib"), "{}", text(&response));
}

#[test]
fn a_setup_error_comes_back_as_a_tool_error() {
    let repo = mini_repo();
    let root = repo.path().to_str().expect("utf8").to_string();
    let mut server = Server::start(repo.path());

    let response = server.call(4, "run_ladder", json!({ "repo": root, "paths": ["docs"] }));
    let result = &response["result"];

    assert_eq!(result["isError"], true);
    assert!(
        text(&response).contains("exit_code: 2"),
        "{}",
        text(&response)
    );
    assert!(
        text(&response).contains("does not exist"),
        "{}",
        text(&response)
    );
}
