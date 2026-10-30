use super::*;
use crate::config::Config;
use crate::test_support::{FakeRunner, MiniRepo};

fn repo() -> MiniRepo {
    MiniRepo::build(None)
}

/// A workspace-target run of the given gates over a fixture repo.
fn gate_run_with<'a>(
    repo: &'a MiniRepo,
    config: &'a Config,
    gates: &'a [Gate],
    files: &'a [String],
) -> GateRun<'a> {
    let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
    GateRun {
        repo: &repo.root,
        target,
        config,
        lang: Lang::Rust,
        files,
        gates,
        scratch: &repo.root,
        baseline_lcov: None,
    }
}

#[test]
fn a_disabled_gate_reports_itself_as_skipped() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [mutation]
        enabled = false
    ",
    ));
    let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
    let run = gate_run_with(&repo, &config, &[Gate::Mutation], &[]);
    let mut out = Vec::new();

    let results = run_gates(&FakeRunner::default(), &run, &mut out, Style::plain());

    assert_eq!(results[0].status, SKIPPED);
    assert!(results[0].contract.contains("enabled = false"));
    let printed = String::from_utf8(out).expect("utf8");
    assert!(
        printed.contains("[1/1] · mutation"),
        "a skipped gate still gets a line: {printed:?}"
    );
    assert!(
        printed.contains("waived"),
        "the line says the config waived it: {printed:?}"
    );
}

#[test]
fn every_gate_reports_a_line_of_its_own() {
    let repo = repo();
    let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
    let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 3 } }] } });
    let runner = FakeRunner::with(&[
        ("tokei", 0, &tokei.to_string()),
        ("cargo test", 0, "test result: ok. 1 passed; 0 failed\n"),
    ])
    .tool("cargo");
    let files = vec!["src/foo.rs".to_string()];
    let run = gate_run_with(
        &repo,
        &config,
        &[Gate::Syntax, Gate::Size, Gate::Analysis, Gate::Tests],
        &files,
    );
    let mut out = Vec::new();

    let results = run_gates(&runner, &run, &mut out, Style::plain());

    assert_eq!(results.len(), 4);
    let printed = String::from_utf8(out).expect("utf8");
    assert!(printed.contains("[1/4] ✓ syntax"));
    assert!(printed.contains("[3/4] ! analysis"));
}

#[test]
fn a_running_gate_announces_itself_on_a_terminal() {
    let repo = repo();
    let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
    let runner = FakeRunner::with(&[("cargo test", 0, "test result: ok. 1 passed; 0 failed\n")])
        .tool("cargo");
    let files = vec!["src/foo.rs".to_string()];
    let run = gate_run_with(&repo, &config, &[Gate::Tests, Gate::Syntax], &files);
    let mut out = Vec::new();

    run_gates(&runner, &run, &mut out, Style::colored());

    let printed = String::from_utf8(out).expect("utf8");
    assert!(
        printed.contains("[1/2] ⋯ tests   running…"),
        "the live line names the gate and its place: {printed:?}"
    );
    assert!(
        printed.contains("[2/2] ⋯ syntax  running…"),
        "and counts forward through the ladder: {printed:?}"
    );
    assert!(
        printed.contains('\r'),
        "the live line is rewritten in place: {printed:?}"
    );
}

#[test]
fn a_piped_run_never_writes_a_live_line() {
    let repo = repo();
    let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
    let runner = FakeRunner::with(&[("cargo test", 0, "test result: ok. 1 passed; 0 failed\n")])
        .tool("cargo");
    let files = vec!["src/foo.rs".to_string()];
    let run = gate_run_with(&repo, &config, &[Gate::Tests], &files);
    let mut out = Vec::new();

    run_gates(&runner, &run, &mut out, Style::plain());

    let printed = String::from_utf8(out).expect("utf8");
    assert!(!printed.contains('\r'), "no rewriting without a terminal");
    assert!(!printed.contains("running…"));
    assert!(printed.contains("[1/1] ✓ tests"));
}

#[test]
fn a_nested_target_is_measured_under_its_own_directory() {
    let repo = repo();
    std::fs::create_dir_all(repo.root.join("cli/src")).expect("dir");
    std::fs::write(repo.root.join("cli/src/foo.rs"), "").expect("file");
    let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
    let document = serde_json::json!({
        "spaces": [{ "name": "f", "kind": "function", "metrics": { "loc": { "sloc": 5 } } }]
    });
    let tokei = serde_json::json!({ "Rust": { "reports": [{ "name": "src/foo.rs", "stats": { "code": 10 } }] } });
    let runner = FakeRunner::with(&[
        ("rust-code-analysis-cli", 0, &document.to_string()),
        ("tokei", 0, &tokei.to_string()),
    ])
    .tool("cargo");
    let target = Target::package_target("cli", true, "Cargo.toml");
    let gates = [Gate::Size];
    let files = vec!["src/foo.rs".to_string()];
    let run = GateRun {
        gates: &gates,
        ..crate::test_support::gate_run(&repo.root, &target, &config, &files)
    };

    let results = run_gates(&runner, &run, &mut Vec::new(), Style::plain());

    assert_eq!(results[0].status, crate::report::PASS);
    assert!(
        results[0].summary.contains("worst function 5 sloc"),
        "the target's own file was never measured: {}",
        results[0].summary
    );
}

#[test]
fn units_are_only_measured_for_the_gates_that_need_them() {
    let repo = repo();
    // The file must exist, or the source filter would mask the gate set.
    std::fs::create_dir_all(repo.root.join("src")).expect("dir");
    std::fs::write(repo.root.join("src/foo.rs"), "").expect("file");
    let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
    let runner = FakeRunner::with(&[("rust-code-analysis-cli", 1, "")]).tool("cargo");
    let files = vec!["src/foo.rs".to_string()];
    let run = gate_run_with(&repo, &config, &[Gate::Syntax], &files);

    run_gates(&runner, &run, &mut Vec::new(), Style::plain());

    assert!(!runner.called_with("rust-code-analysis-cli"));
}
