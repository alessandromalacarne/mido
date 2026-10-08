use super::*;
use crate::test_support::gate_run;

/// A frontend crate that browser-tests, so its target declares two test
/// commands: the native logic tests and the browser journeys.
fn browser_tested_frontend(repo: &MiniRepo) {
    let manifest = repo.root.join("frontend/Cargo.toml");
    let text = std::fs::read_to_string(&manifest).expect("manifest");
    std::fs::write(
        &manifest,
        format!("{text}\n[dev-dependencies]\nwasm-bindgen-test = \"=0.3.73\"\n"),
    )
    .expect("manifest");
}

/// A gate run over the standalone `frontend` crate — the shape whose target
/// declares more than one test command.
fn frontend_run<'a>(repo: &'a MiniRepo, config: &'a Config, files: &'a [String]) -> GateRun<'a> {
    let target = Box::leak(Box::new(crate::targets::Target::crate_target(
        "frontend",
        false,
        crate::lang::rust::MANIFEST,
    )));

    gate_run(&repo.root, target, config, files)
}

/// The measured rust file the mutation gate needs on every run.
fn rust_files() -> Vec<String> {
    vec!["src/foo.rs".to_string()]
}

/// The union fixture: the native suite catches the one mutant it can see and
/// misses five; the browser journeys get those survivors, catch four, and leave
/// one equivalent mutant alive.
fn native_then_browser() -> MutationStub {
    MutationStub::new(vec![
        Some(Pass {
            total: 6,
            caught: 1,
            missed: 5,
            ..Pass::default()
        }),
        Some(Pass {
            total: 5,
            caught: 4,
            missed: 1,
            skipped: vec!["src/foo.rs:1:1: replace + with - in a"],
            survivors: vec!["src/foo.rs:2:2: replace a with b in b"],
            ..Pass::default()
        }),
    ])
}

#[test]
fn every_test_command_gets_a_pass_and_the_union_decides_the_verdict() {
    let repo = repo();
    browser_tested_frontend(&repo);
    let config = config_for(&repo);
    let files = rust_files();
    let stub = native_then_browser();

    let run = frontend_run(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, PASS);
    assert!(
        result.summary.contains("83.3% killed (min 70)"),
        "{}",
        result.summary
    );
    assert!(
        stub.called_with("-- --target wasm32-unknown-unknown"),
        "the browser suite drives its own pass"
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("src/foo.rs:2:2")),
        "the survivors of the deciding pass are listed: {:?}",
        result.details
    );
    assert!(
        result.details.iter().any(|line| line.contains(
            "pass `cargo test --target wasm32-unknown-unknown`: 4 caught, 1 missed, 1 skipped"
        )),
        "the evidence names each pass and what it caught: {:?}",
        result.details
    );
    assert!(
        result
            .details
            .iter()
            .any(|line| line.contains("pass `cargo test`: 1 caught, 5 missed, 0 skipped")),
        "the earlier pass is named too: {:?}",
        result.details
    );
}

#[test]
fn a_pass_with_nothing_left_keeps_the_verdict_of_the_earlier_passes() {
    let repo = repo();
    browser_tested_frontend(&repo);
    let config = config_for(&repo);
    let files = rust_files();
    let stub = MutationStub::new(vec![
        Some(Pass {
            total: 6,
            caught: 6,
            ..Pass::default()
        }),
        Some(Pass {
            total: 0,
            skipped: vec!["a", "b", "c", "d", "e", "f"],
            ..Pass::default()
        }),
    ]);

    let run = frontend_run(&repo, &config, &files);
    let result = gate_mutation(&stub, &run);

    assert_eq!(result.status, PASS);
    assert!(
        result.summary.contains("100.0% killed (min 70)"),
        "the pass that had nothing to test leaves the caught mutants caught: {}",
        result.summary
    );
}

#[test]
fn the_first_pass_keeps_the_configured_command_without_iterate() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [targets.frontend.mutation]
        command = [\"cargo\", \"mutants\", \"-j2\"]
    ",
    ));
    browser_tested_frontend(&repo);
    let config = config_for(&repo);
    let files = rust_files();
    let stub = MutationStub::new(vec![Some(Pass::default()), Some(Pass::default())]);

    let run = frontend_run(&repo, &config, &files);
    gate_mutation(&stub, &run);

    let calls = stub.mutant_calls();
    assert_eq!(calls.len(), 2, "one pass per test command: {calls:?}");
    assert!(
        !calls[0].iter().any(|arg| arg == "--iterate"),
        "the first pass runs the configured command as it stands: {:?}",
        calls[0]
    );
    assert!(
        calls[1].iter().any(|arg| arg == "--iterate"),
        "the pass that follows has to skip what the first one caught: {:?}",
        calls[1]
    );
}

#[test]
fn the_declared_test_command_rides_along_on_the_pass() {
    let repo = MiniRepo::build(Some(
        "
        version = 1

        [tests]
        command = [\"cargo\", \"test\", \"--all-features\"]
    ",
    ));
    let config = config_for(&repo);
    let files = rust_files();
    let stub = MutationStub::new(vec![Some(Pass::default())]);

    gate_mutation(&stub, &gate_run_for(&repo, &config, &files));

    assert!(
        stub.called_with("-- --all-features"),
        "the pass tests the mutants with the declared suite"
    );
}
