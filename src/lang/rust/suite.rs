//! libtest output, and the commands the tests gate runs for a target.

use crate::config::{value, Config};
use crate::lang::TestSummary;
use crate::targets::Target;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

fn summary_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^test result: (\w+)\. (\d+) passed; (\d+) failed").expect("valid pattern")
    })
}

fn failure_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^\s*(?:test\s+)?([A-Za-z0-9_:]+) \.\.\. FAILED").expect("valid pattern")
    })
}

/// Every `test result:` line, summed; `None` when the output printed none.
pub fn test_summary(output: &str) -> Option<TestSummary> {
    let reported: Vec<(i64, i64)> = summary_pattern()
        .captures_iter(output)
        .map(|captures| {
            (
                captures[2].parse().unwrap_or_default(),
                captures[3].parse().unwrap_or_default(),
            )
        })
        .collect();
    if reported.is_empty() {
        return None;
    }

    let mut failed_names: Vec<String> = failure_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .collect();
    failed_names.sort();
    failed_names.dedup();

    Some(TestSummary {
        passed: reported.iter().map(|(passed, _)| passed).sum(),
        failed: reported.iter().map(|(_, failed)| failed).sum(),
        failed_names,
    })
}

/// The commands the tests gate runs for this target.
///
/// An explicit `[targets.<name>.tests] command` wins, then the root `[tests]
/// command` for the workspace and its members. A standalone crate derives its
/// own — including the wasm32 run when it browser-tests through
/// `wasm-bindgen-test`.
pub fn test_commands(config: &Config, target: &Target, repo: &Path) -> Vec<Vec<String>> {
    if let Some(configured) = config.argv("tests", "command", target) {
        return vec![configured];
    }

    let mut commands =
        vec![super::fallback_argv("tests", "command").expect("rust has a bare test command")];
    if !target.workspace_member && target.manifest.is_some() && uses_wasm_bindgen_test(repo, target)
    {
        commands.push(
            ["cargo", "test", "--target", "wasm32-unknown-unknown"]
                .iter()
                .map(|arg| (*arg).to_string())
                .collect(),
        );
    }
    commands
}

pub fn uses_wasm_bindgen_test(repo: &Path, target: &Target) -> bool {
    let Some(manifest) = &target.manifest else {
        return false;
    };
    let path = repo.join(manifest);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(data) = text.parse::<toml::Table>() else {
        return false;
    };
    data.get("dev-dependencies")
        .and_then(value::as_table)
        .map(|dependencies| dependencies.contains_key("wasm-bindgen-test"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::test_support::MiniRepo;

    fn repo(config: Option<&str>) -> MiniRepo {
        MiniRepo::build(config)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root, &Lang::Rust).expect("config loads")
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn a_summary_line_is_read_into_passed_and_failed() {
        let summary =
            test_summary("test result: ok. 41 passed; 0 failed; 0 ignored\n").expect("a summary");

        assert_eq!(summary.passed, 41);
        assert_eq!(summary.failed, 0);
        assert!(summary.failed_names.is_empty());
    }

    #[test]
    fn failing_tests_are_named() {
        let output = "test thing::works ... ok\ntest thing::breaks ... FAILED\n\ntest result: FAILED. 1 passed; 1 failed\n";
        let summary = test_summary(output).expect("a summary");

        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failed_names, vec!["thing::breaks"]);
    }

    #[test]
    fn output_without_a_summary_line_is_nothing() {
        assert_eq!(test_summary("cargo: command not found"), None);
    }

    #[test]
    fn the_workspace_inherits_the_root_test_command() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\", \"--all-features\"]
        ",
        ));

        assert_eq!(
            test_commands(
                &config_for(&repo),
                &Target::workspace_target(crate::lang::rust::MANIFEST),
                &repo.root
            ),
            vec![argv(&["cargo", "test", "--all-features"])]
        );
    }

    #[test]
    fn a_standalone_crate_does_not_inherit_the_root_test_command() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\", \"--all-features\"]

            [targets.frontend.tests]
            command = [\"cargo\", \"test\", \"--target\", \"wasm32-unknown-unknown\"]
        ",
        ));
        let config = config_for(&repo);
        let frontend = Target::crate_target("frontend", false, crate::lang::rust::MANIFEST);

        assert_eq!(
            test_commands(&config, &frontend, &repo.root),
            vec![argv(&[
                "cargo",
                "test",
                "--target",
                "wasm32-unknown-unknown"
            ])]
        );
    }

    #[test]
    fn a_standalone_crate_without_its_own_command_falls_back_to_cargo_test() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\", \"--all-features\"]
        ",
        ));
        let config = config_for(&repo);

        assert_eq!(
            test_commands(
                &config,
                &Target::crate_target("desktop", false, crate::lang::rust::MANIFEST),
                &repo.root
            ),
            vec![argv(&["cargo", "test"])]
        );
    }

    #[test]
    fn a_crate_that_browser_tests_gets_a_second_wasm_command() {
        let repo = repo(None);
        let manifest = repo.root.join("frontend/Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("manifest");
        std::fs::write(
            &manifest,
            format!("{text}\n[dev-dependencies]\nwasm-bindgen-test = \"=0.3.73\"\n"),
        )
        .expect("manifest");
        let config = config_for(&repo);

        assert_eq!(
            test_commands(
                &config,
                &Target::crate_target("frontend", false, crate::lang::rust::MANIFEST),
                &repo.root
            ),
            vec![
                argv(&["cargo", "test"]),
                argv(&["cargo", "test", "--target", "wasm32-unknown-unknown"])
            ]
        );
    }

    #[test]
    fn a_crate_without_browser_tests_runs_once() {
        let repo = repo(None);

        assert_eq!(
            test_commands(
                &config_for(&repo),
                &Target::crate_target("desktop", false, crate::lang::rust::MANIFEST),
                &repo.root
            ),
            vec![argv(&["cargo", "test"])]
        );
    }

    #[test]
    fn a_crate_that_does_not_browser_test_through_the_macro_runs_once() {
        let repo = repo(None);
        let manifest = repo.root.join("frontend/Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("manifest");
        std::fs::write(
            &manifest,
            format!("{text}\n[dev-dependencies]\nsome-other-helper = \"1\"\n"),
        )
        .expect("manifest");

        assert_eq!(
            test_commands(
                &config_for(&repo),
                &Target::crate_target("frontend", false, crate::lang::rust::MANIFEST),
                &repo.root
            ),
            vec![argv(&["cargo", "test"])]
        );
    }
}
