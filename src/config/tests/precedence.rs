//! How the layers compose: module defaults, the repo file, and target sections.

use super::{argv, load, repo};
use crate::config::Threshold;
use crate::targets::Target;
use crate::test_support::MiniRepo;

#[test]
fn target_section_overrides_the_root_threshold() {
    let repo = repo(
        "
            version = 1

            [size]
            file_loc = { warn = 300, fail = 500 }

            [targets.frontend.size]
            file_loc = { warn = 100, fail = 200 }
        ",
    );
    let config = load(&repo).expect("config loads");

    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(1, 2), Some("frontend")),
        Threshold::new(100, 200)
    );
    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(1, 2), Some("workspace")),
        Threshold::new(300, 500)
    );
}

#[test]
fn thresholds_are_inherited_by_a_crate_target() {
    let repo = repo(
        "
            version = 1

            [mutation]
            kill_rate_min = 85
        ",
    );
    let config = load(&repo).expect("config loads");

    assert_eq!(
        config.float("mutation", "kill_rate_min", 70.0, Some("frontend")),
        85.0
    );
}

#[test]
fn standalone_crate_does_not_inherit_the_root_lint_command() {
    let repo = repo(
        "
            version = 1

            [syntax]
            lint = [\"cargo\", \"clippy\", \"--all-targets\", \"--all-features\"]
        ",
    );
    let config = load(&repo).expect("config loads");
    let frontend = Target::crate_target("frontend", false, "Cargo.toml");

    assert_eq!(config.argv("syntax", "lint", &frontend), None);
}

#[test]
fn workspace_member_inherits_the_root_lint_command() {
    let repo = repo(
        "
            version = 1

            [syntax]
            lint = [\"cargo\", \"clippy\", \"--all-targets\", \"--all-features\"]
        ",
    );
    let config = load(&repo).expect("config loads");
    let lib = Target::crate_target("lib", true, "Cargo.toml");

    assert_eq!(
        config.argv("syntax", "lint", &lib),
        Some(argv(&[
            "cargo",
            "clippy",
            "--all-targets",
            "--all-features"
        ]))
    );
}

#[test]
fn target_section_wins_over_the_root_command() {
    let repo = repo(
        "
            version = 1

            [syntax]
            lint = [\"cargo\", \"clippy\", \"--all-features\"]

            [targets.frontend.syntax]
            lint = [\"cargo\", \"clippy\", \"--target\", \"wasm32-unknown-unknown\"]
        ",
    );
    let config = load(&repo).expect("config loads");
    let frontend = Target::crate_target("frontend", false, "Cargo.toml");

    assert_eq!(
        config.argv("syntax", "lint", &frontend),
        Some(argv(&[
            "cargo",
            "clippy",
            "--target",
            "wasm32-unknown-unknown"
        ]))
    );
}

#[test]
fn a_target_section_is_merged_into_the_root_section() {
    let repo = repo(
        "
            version = 1

            [size]
            file_loc = { warn = 300, fail = 500 }
            nesting = { warn = 3, fail = 4 }

            [targets.frontend.size]
            file_loc = { warn = 100, fail = 200 }
        ",
    );
    let config = load(&repo).expect("config loads");

    let frontend = config.section("size", Some("frontend"));
    assert_eq!(frontend["file_loc"]["fail"], toml::Value::Integer(200));
    assert_eq!(frontend["nesting"]["fail"], toml::Value::Integer(4));

    let workspace = config.section("size", Some("workspace"));
    assert_eq!(workspace["file_loc"]["fail"], toml::Value::Integer(500));
}

#[test]
fn module_defaults_fill_a_config_that_is_silent() {
    let repo = MiniRepo::build(None);
    let config = load(&repo).expect("defaults load");
    let workspace = Target::workspace_target("Cargo.toml");

    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(1, 2), None),
        Threshold::new(300, 500)
    );
    assert_eq!(config.float("analysis", "mi_min", 0.0, None), 20.0);
    assert_eq!(
        config.argv("tests", "command", &workspace),
        Some(argv(&["cargo", "test", "--all-features"]))
    );
}

#[test]
fn the_file_overrides_a_module_default_key_by_key() {
    let repo = repo(
        "
            version = 1

            [size]
            file_loc = { warn = 100, fail = 200 }
        ",
    );
    let config = load(&repo).expect("config loads");

    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(1, 2), None),
        Threshold::new(100, 200),
        "the file's value wins"
    );
    assert_eq!(
        config.threshold("size", "nesting", Threshold::new(1, 2), None),
        Threshold::new(3, 4),
        "untouched keys still come from the module defaults"
    );
}

#[test]
fn a_standalone_crate_never_inherits_module_default_commands() {
    let repo = MiniRepo::build(None);
    let config = load(&repo).expect("defaults load");
    let frontend = Target::crate_target("frontend", false, "Cargo.toml");

    assert_eq!(
        config.argv("tests", "command", &frontend),
        None,
        "the gate falls back to the module's bare command instead"
    );
}

#[test]
fn a_standalone_crate_still_reads_module_default_thresholds() {
    let repo = MiniRepo::build(None);
    let config = load(&repo).expect("defaults load");

    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(1, 2), Some("frontend")),
        Threshold::new(300, 500)
    );
}
