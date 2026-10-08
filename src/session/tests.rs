use super::*;
use crate::cli::parse_from;
use crate::gate::GATES;
use crate::lang::Lang;
use crate::test_support::{config_for, FakeRunner, MiniRepo};

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
fn a_bare_run_selects_the_workspace() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let targets = Lang::Rust.detect_targets(&repo.root, &config);

    let selected = select_targets(&parse_from(&[]), &repo.root, &config, Lang::Rust, &targets)
        .expect("selection is fine");

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "workspace");
    assert_eq!(selected[0].path, "");
}

#[test]
fn minus_p_selects_packages_by_name() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let targets = Lang::Rust.detect_targets(&repo.root, &config);

    let selected = select_targets(
        &parse_from(&["-p", "lib"]),
        &repo.root,
        &config,
        Lang::Rust,
        &targets,
    )
    .expect("selection is fine");

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "lib");
    assert_eq!(selected[0].path, "lib");
}

#[test]
fn repeating_a_package_selects_it_once() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let targets = Lang::Rust.detect_targets(&repo.root, &config);

    let selected = select_targets(
        &parse_from(&["-p", "lib", "-p", "lib", "-p", "frontend"]),
        &repo.root,
        &config,
        Lang::Rust,
        &targets,
    )
    .expect("selection is fine");

    let names: Vec<&str> = selected.iter().map(|target| target.name.as_str()).collect();
    assert_eq!(names, vec!["lib", "frontend"]);
}

#[test]
fn an_unknown_package_is_a_setup_error() {
    let repo = MiniRepo::build(None);
    let config = config_for(&repo);
    let targets = Lang::Rust.detect_targets(&repo.root, &config);

    let message = select_targets(
        &parse_from(&["-p", "nope"]),
        &repo.root,
        &config,
        Lang::Rust,
        &targets,
    )
    .expect_err("unknown package")
    .render();

    assert!(message.contains("package `nope` not found"), "{message}");
    assert!(message.contains("frontend"), "{message}");
}
