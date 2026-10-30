use super::*;
use crate::lang::Lang;
use crate::test_support::MiniRepo;

mod precedence;

fn load(repo: &MiniRepo) -> Result<Config, GuardrailsError> {
    Config::load(&repo.root, &Lang::Rust)
}

fn repo(config: &str) -> MiniRepo {
    MiniRepo::build(Some(config))
}

#[test]
fn repo_config_declares_every_gate() {
    let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR")), &Lang::Rust)
        .expect("repo config loads");

    for gate in keys::GATES {
        assert!(
            config.data().contains_key(gate),
            ".mido.toml has no [{gate}] section"
        );
    }
}

#[test]
fn the_mutation_diff_switch_reads_the_file_over_the_module_default() {
    let bare = MiniRepo::build(None);
    assert!(
        load(&bare)
            .expect("config loads")
            .flag("mutation", "in_diff", true, Some("workspace")),
        "the module default scopes a mutation run to the change"
    );

    let repo = repo(
        "
            version = 1

            [mutation]
            in_diff = false

            [targets.frontend.mutation]
            in_diff = true
        ",
    );
    let config = load(&repo).expect("config loads");

    assert!(!config.flag("mutation", "in_diff", true, Some("workspace")));
    assert!(config.flag("mutation", "in_diff", true, Some("frontend")));
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
fn missing_config_falls_back_to_the_module_defaults() {
    let repo = MiniRepo::build(None);

    let config = load(&repo).expect("defaults load");

    assert_eq!(config.source(), "rust built-in defaults");
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
fn the_failure_cap_comes_from_the_file_or_the_module_defaults() {
    let repo = repo(
        "
            version = 1

            [failure]
            max_attempts_per_gate = 5
        ",
    );
    assert_eq!(load(&repo).expect("config loads").attempts_cap(), Some(5));

    let bare = MiniRepo::build(None);
    assert_eq!(load(&bare).expect("defaults load").attempts_cap(), Some(3));
}
