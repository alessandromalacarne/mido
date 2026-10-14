use super::report::REPORT_FILE;
use super::*;
use crate::gates::mutation::gate_mutation;
use crate::process::{Command, Outcome, Runner};
use crate::report::{INCOMPLETE, PASS};
use crate::test_support::{js_config_for, js_gate_run_for, FakeRunner, MiniJsRepo};

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

#[test]
fn a_test_command_carries_no_stryker_arguments() {
    assert_eq!(test_args(&argv(&["npm", "test"])), None);
}

#[test]
fn a_pass_carries_midos_own_config_positionally() {
    let args = pass_args(
        &argv(&["npx", "--no-install", "stryker", "run"]),
        &[],
        false,
        Path::new("/run/mutants"),
        None,
    )
    .expect("a pass");

    assert_eq!(
        args,
        argv(&[
            "npx",
            "--no-install",
            "stryker",
            "run",
            "/run/mutants/stryker-mido.config.json"
        ])
    );
}

#[test]
fn a_change_scoped_run_is_refused() {
    let error = pass_args(
        &argv(&["npx", "--no-install", "stryker", "run"]),
        &[],
        false,
        Path::new("/run"),
        Some(Path::new("/run/guardrails-mutants-workspace.diff")),
    )
    .expect_err("stryker has no --in-diff");

    assert!(error.render().contains("has no change scope"));
}

#[test]
fn a_command_that_names_its_own_config_is_refused() {
    let error = pass_args(
        &argv(&[
            "npx",
            "--no-install",
            "stryker",
            "run",
            "stryker.config.json",
        ]),
        &[],
        false,
        Path::new("/run"),
        None,
    )
    .expect_err("a conflict");

    assert!(error.render().contains("names its own stryker config"));
}

#[test]
fn prepare_extends_the_repos_config_when_there_is_one() {
    let repo = MiniJsRepo::build(None);
    std::fs::write(
        repo.root.join("stryker.config.json"),
        "{ \"mutate\": [\"src/**/*.js\"] }\n",
    )
    .expect("config");
    let output = repo.root.join("out");

    prepare(&output, &repo.root).expect("prepared");

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join(CONFIG_FILE)).expect("config"))
            .expect("json");
    assert_eq!(
        config["extends"],
        serde_json::json!(repo.root.join("stryker.config.json").to_string_lossy())
    );
    assert_eq!(config["reporters"], serde_json::json!(["json"]));
    assert_eq!(
        config["jsonReporter"]["fileName"],
        serde_json::json!(output.join(REPORT_FILE).to_string_lossy())
    );
}

#[test]
fn prepare_without_a_repo_config_carries_no_extends() {
    let repo = MiniJsRepo::build(None);
    let output = repo.root.join("out");

    prepare(&output, &repo.root).expect("prepared");

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join(CONFIG_FILE)).expect("config"))
            .expect("json");
    assert!(config.get("extends").is_none());
}

#[test]
fn repo_configs_are_discovered_in_stryker_order() {
    let repo = MiniJsRepo::build(None);
    assert_eq!(repo_config(&repo.root), None);

    std::fs::write(repo.root.join("stryker.config.mjs"), "export default {}\n").expect("config");
    assert_eq!(
        repo_config(&repo.root),
        Some(repo.root.join("stryker.config.mjs"))
    );
}

/// A runner that plays stryker: on the pass it writes the report the generated
/// config points at — the mutants the tool "tested".
struct StrykerStub {
    runner: FakeRunner,
    mutants: Vec<(&'static str, &'static str)>,
    writes_report: bool,
}

impl StrykerStub {
    fn with(mutants: Vec<(&'static str, &'static str)>) -> Self {
        Self {
            runner: FakeRunner::default(),
            mutants,
            writes_report: true,
        }
    }

    fn dying() -> Self {
        Self {
            runner: FakeRunner::default(),
            mutants: Vec::new(),
            writes_report: false,
        }
    }
}

impl Runner for StrykerStub {
    fn exec(&self, command: &Command) -> Outcome {
        if !self.writes_report {
            return Outcome::new(1, "", "stryker crashed before writing a report");
        }
        if let Some(config_path) = command.args.last().filter(|arg| arg.ends_with(CONFIG_FILE)) {
            let config: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(config_path).expect("the pass carries mido's config"),
            )
            .expect("config json");
            let report = config["jsonReporter"]["fileName"]
                .as_str()
                .expect("the config names the report");
            write_report(Path::new(report), &self.mutants);
        }
        self.runner.exec(command)
    }

    fn has(&self, tool: &str) -> bool {
        self.runner.has(tool)
    }
}

fn write_report(path: &Path, mutants: &[(&str, &str)]) {
    let mutants: Vec<serde_json::Value> = mutants
        .iter()
        .enumerate()
        .map(|(index, (status, mutator))| {
            serde_json::json!({
                "id": index,
                "status": status,
                "mutatorName": mutator,
                "replacement": "x",
                "location": { "start": { "line": 1, "column": index as i64 + 1 } },
            })
        })
        .collect();
    let report = serde_json::json!({
        "files": { "src/calc.js": { "language": "javascript", "source": "", "mutants": mutants } }
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("report dir");
    }
    std::fs::write(path, report.to_string()).expect("report");
}

fn js_files() -> Vec<String> {
    vec!["src/calc.js".to_string()]
}

#[test]
fn a_finished_js_run_decides_the_verdict() {
    let repo = MiniJsRepo::build(None);
    let config = js_config_for(&repo);
    let files = js_files();
    let mut mutants = vec![("Killed", "ConditionalExpression"); 8];
    mutants.push(("Survived", "ArithmeticOperator"));
    mutants.push(("Survived", "StringLiteral"));
    let stub = StrykerStub::with(mutants);

    let result = gate_mutation(&stub, &js_gate_run_for(&repo, &config, &files));

    assert_eq!(result.status, PASS);
    assert!(
        result.summary.contains("80.0% killed (min 70)"),
        "{}",
        result.summary
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.starts_with("MISSED  src/calc.js:1:9")),
        "the survivors are named: {:?}",
        result.details
    );
    assert!(result.contract.contains("stryker-mido.config.json"));
}

#[test]
fn ignored_mutants_stay_out_of_the_total() {
    let repo = MiniJsRepo::build(None);
    let config = js_config_for(&repo);
    let files = js_files();
    let mut mutants = vec![("Killed", "BlockStatement"); 4];
    mutants.extend(vec![("Ignored", "StringLiteral"); 3]);
    let stub = StrykerStub::with(mutants);

    let result = gate_mutation(&stub, &js_gate_run_for(&repo, &config, &files));

    assert_eq!(result.status, PASS);
    assert!(
        result.details[0].contains("4 mutants: 4 caught"),
        "ignored mutants are not mutants this run judged: {:?}",
        result.details[0]
    );
}

#[test]
fn a_timed_out_mutant_is_incomplete() {
    let repo = MiniJsRepo::build(None);
    let config = js_config_for(&repo);
    let files = js_files();
    let stub = StrykerStub::with(vec![
        ("Killed", "BlockStatement"),
        ("Timeout", "ArithmeticOperator"),
    ]);

    let result = gate_mutation(&stub, &js_gate_run_for(&repo, &config, &files));

    assert_eq!(result.status, INCOMPLETE, "a timeout is not a verdict");
    assert!(
        result
            .details
            .iter()
            .any(|line| line.starts_with("TIMEOUT  ")),
        "{:?}",
        result.details
    );
}

#[test]
fn an_unclassified_status_is_incomplete() {
    let repo = MiniJsRepo::build(None);
    let config = js_config_for(&repo);
    let files = js_files();
    let stub = StrykerStub::with(vec![
        ("Killed", "BlockStatement"),
        ("RuntimeError", "ArithmeticOperator"),
    ]);

    let result = gate_mutation(&stub, &js_gate_run_for(&repo, &config, &files));

    assert_eq!(result.status, INCOMPLETE);
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("1 mutants did not report an outcome")),
        "{:?}",
        result.details
    );
}

#[test]
fn a_stale_report_from_an_earlier_run_is_never_read() {
    let repo = MiniJsRepo::build(None);
    let config = js_config_for(&repo);
    let files = js_files();
    let run = js_gate_run_for(&repo, &config, &files);
    // A leftover report where the gate keeps its state: were it read, this
    // perfect pass would decide the verdict.
    write_report(
        &run.scratch
            .join("guardrails-mutants-workspace")
            .join(REPORT_FILE),
        &[("Killed", "BlockStatement"); 4],
    );
    let stub = StrykerStub::dying();

    let result = gate_mutation(&stub, &run);

    assert_eq!(
        result.status, INCOMPLETE,
        "the verdict must be earned by this run, not an earlier one"
    );
    assert!(result.summary.contains("produced no report"));
}
