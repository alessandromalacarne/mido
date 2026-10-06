use super::*;
use crate::cli::parse_from;
use crate::gate::GATES;
use crate::lang::Lang;
use crate::test_support::{config_for, session_for, FakeRunner, MiniRepo};

mod output;
mod run;

#[test]
fn a_gate_name_that_does_not_exist_never_reaches_the_ladder() {
    let rejected = crate::cli::try_parse_from(&["--gate", "lint"]);

    assert!(rejected.is_err(), "an unknown gate is not a gate");
    assert_eq!(rejected.expect_err("rejected").exit_code(), 2);
}

#[test]
fn gate_selection_narrows_the_ladder() {
    let args = parse_from(&["--gate", "size", "--gate", "syntax"]);

    assert_eq!(
        args.gates
            .iter()
            .map(|gate| gate.name())
            .collect::<Vec<_>>(),
        vec!["size", "syntax"]
    );
}

#[test]
fn a_run_without_a_gate_selection_uses_all_six() {
    let repo = MiniRepo::build(None);
    let session = build_session(
        &parse_from(&["--repo", "."]),
        &repo.root,
        config_for(&repo),
        &FakeRunner::default(),
        Lang::Rust,
    )
    .expect("session");

    assert_eq!(session.gates.len(), GATES.len());
}

#[test]
fn a_target_owning_none_of_the_diff_is_skipped_with_a_note() {
    let repo = MiniRepo::build(None);
    let session = Session {
        changed: vec!["lib/src/foo.rs".to_string()],
        ..session_for(&repo, config_for(&repo))
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut io = Io {
        out: &mut out,
        err: &mut err,
        style: crate::style::Style::plain(),
    };

    let results = run_target(
        &FakeRunner::default(),
        &session,
        &Target::crate_target("frontend", false, "Cargo.toml"),
        &mut io,
    )
    .expect("no setup error");

    assert!(results.is_none());
    assert!(
        String::from_utf8_lossy(&out).contains("no changed file belongs to frontend (frontend/)")
    );
}

#[test]
fn a_check_that_never_ran_is_reported_as_not_ship_ready() {
    let repo = MiniRepo::build(None);
    let session = Session {
        changed: vec!["README.md".to_string()],
        gates: vec![Gate::Tests],
        ..session_for(&repo, config_for(&repo))
    };
    let targets = Lang::Rust.detect_targets(&repo.root, &session.config);
    let mut out = Vec::new();

    let selected = select_targets(
        &parse_from(&["--repo", "."]),
        &session,
        &targets,
        &mut out,
        crate::style::Style::plain(),
    )
    .expect("selection is fine");

    assert!(selected.is_none());
    assert!(String::from_utf8_lossy(&out).contains("nothing to measure"));
}

#[test]
fn every_target_can_be_selected_with_all() {
    let repo = MiniRepo::build(None);
    let session = Session {
        changed: vec!["lib/src/foo.rs".to_string()],
        gates: vec![Gate::Tests],
        ..session_for(&repo, config_for(&repo))
    };
    let targets = Lang::Rust.detect_targets(&repo.root, &session.config);

    let selected = select_targets(
        &parse_from(&["--all"]),
        &session,
        &targets,
        &mut Vec::new(),
        crate::style::Style::plain(),
    )
    .expect("selection is fine")
    .expect("targets selected");

    assert!(selected.iter().any(|target| target.name == "workspace"));
    assert!(selected.iter().any(|target| target.name == "frontend"));
}

#[test]
fn the_session_records_how_the_target_was_chosen() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let runner = FakeRunner::default();

    let auto = build_session(
        &parse_from(&["--repo", "."]),
        &repo.root,
        config.clone(),
        &runner,
        Lang::Rust,
    )
    .expect("session");
    assert_eq!(auto.selection, "auto");

    let requested = build_session(
        &parse_from(&["--repo", ".", "frontend"]),
        &repo.root,
        config,
        &runner,
        Lang::Rust,
    )
    .expect("session");
    assert_eq!(requested.selection, "requested");
}
