use super::*;
use crate::test_support::MiniRepo;

fn load(repo: &MiniRepo) -> Result<Config, GuardrailsError> {
    Config::load(&repo.root)
}

fn repo(config: &str) -> MiniRepo {
    MiniRepo::build(Some(config))
}

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn repo_config_is_valid() {
    let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repo config loads");

    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(1, 2), None),
        Threshold::new(300, 500)
    );
}

#[test]
fn repo_config_declares_every_gate() {
    let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repo config loads");

    for gate in keys::GATES {
        assert!(
            config.data().contains_key(gate),
            ".mido.toml has no [{gate}] section"
        );
    }
}

#[test]
fn lint_command_comes_from_the_config() {
    let repo = repo(
        "
            version = 1

            [syntax]
            lint = [\"cargo\", \"clippy\", \"--\", \"-D\", \"warnings\"]
        ",
    );

    let config = load(&repo).expect("config loads");

    assert_eq!(
        config.argv("syntax", "lint", &Target::workspace_target()),
        Some(argv(&["cargo", "clippy", "--", "-D", "warnings"]))
    );
}

#[test]
fn unknown_key_is_a_config_error_naming_key_and_section() {
    let repo = repo(
        "
            version = 1

            [analysis]
            mi_min = 20
            min_mi = 20
        ",
    );

    let error = load(&repo).expect_err("config is rejected");
    let message = error.render();

    assert!(message.contains("min_mi"));
    assert!(message.contains("[analysis]"));
    assert!(message.contains("line 6"));
    assert!(message.contains("mi_min"));
}

#[test]
fn misspelled_section_is_a_config_error() {
    let repo = repo(
        "
            version = 1

            [test]
            command = [\"cargo\", \"test\"]
        ",
    );

    let message = load(&repo).expect_err("config is rejected").render();

    assert!(message.contains("[test]"));
    assert!(message.contains("tests"));
}

#[test]
fn unknown_key_inside_a_target_section_is_a_config_error() {
    let repo = repo(
        "
            version = 1

            [targets.frontend]
            path = \"frontend\"
            paths = \"frontend\"
        ",
    );

    let message = load(&repo).expect_err("config is rejected").render();

    assert!(message.contains("paths"));
}

#[test]
fn unknown_threshold_key_is_a_config_error() {
    let repo = repo(
        "
            version = 1

            [size]
            file_loc = { warn = 300, maximum = 500 }
        ",
    );

    let message = load(&repo).expect_err("config is rejected").render();

    assert!(message.contains("maximum"));
    assert!(message.contains("warn, fail"));
}

#[test]
fn missing_version_is_a_warning_not_an_error() {
    let repo = repo(
        "
            [tests]
            command = [\"cargo\", \"test\"]
        ",
    );

    let config = load(&repo).expect("config loads");

    assert!(config
        .warnings()
        .iter()
        .any(|warning| warning.contains("version")));
}

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
    let frontend = Target::crate_target("frontend", false);

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
    let lib = Target::crate_target("lib", true);

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
    let frontend = Target::crate_target("frontend", false);

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
fn script_key_names_the_declared_runner() {
    let repo = repo(
        "
            version = 1
            script = \"scripts/guardrails.py\"
        ",
    );
    std::fs::write(
        repo.root.join("scripts/guardrails.py"),
        "# the ladder runner\n",
    )
    .expect("runner");

    assert_eq!(
        load(&repo).expect("config loads").script().as_deref(),
        Some("scripts/guardrails.py")
    );
}

#[test]
fn script_key_is_optional() {
    let repo = repo("version = 1\n");

    assert_eq!(load(&repo).expect("config loads").script(), None);
}

#[test]
fn script_key_pointing_at_a_missing_file_is_a_config_error() {
    let repo = repo(
        "
            version = 1
            script = \"scripts/ghost.py\"
        ",
    );

    let message = load(&repo).expect_err("config is rejected").render();

    assert!(message.contains("script"));
    assert!(message.contains("scripts/ghost.py"));
    assert!(message.contains("line 3"));
}

#[test]
fn script_key_must_be_a_string() {
    let repo = repo(
        "
            version = 1
            script = 3
        ",
    );

    assert!(load(&repo)
        .expect_err("config is rejected")
        .render()
        .contains("script"));
}

#[test]
fn missing_config_falls_back_to_built_in_defaults() {
    let repo = MiniRepo::build(None);

    let config = load(&repo).expect("defaults load");

    assert_eq!(config.source(), "built-in defaults");
    assert_eq!(
        config.threshold("size", "file_loc", Threshold::new(300, 500), None),
        Threshold::new(300, 500)
    );
    assert!(config
        .warnings()
        .iter()
        .any(|warning| warning.contains("no ")));
}

#[test]
fn an_unparseable_config_is_an_error() {
    let repo = repo("version = \n");

    assert!(load(&repo)
        .expect_err("config is rejected")
        .render()
        .contains("not parseable"));
}

#[test]
fn source_names_the_config_file() {
    let repo = repo("version = 1\n");

    assert_eq!(load(&repo).expect("config loads").source(), "`.mido.toml`");
}

#[test]
fn enabled_is_true_unless_the_gate_is_switched_off() {
    let repo = repo(
        "
            version = 1

            [mutation]
            enabled = false
        ",
    );
    let config = load(&repo).expect("config loads");

    assert!(!config.enabled("mutation", None));
    assert!(config.enabled("tests", None));
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
fn commands_are_read_as_argv() {
    let repo = repo(
        "
            version = 1

            [tests]
            command = [\"cargo\", \"test\", \"--all-features\"]
        ",
    );
    let config = load(&repo).expect("config loads");
    let target = Target::workspace_target();

    assert_eq!(
        config.argv("tests", "command", &target),
        Some(argv(&["cargo", "test", "--all-features"]))
    );
}
