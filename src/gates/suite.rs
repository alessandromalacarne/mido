//! Gate 4 — the full suite.

use crate::config::{value, Config};
use crate::gates::fix_hints;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, PASS};
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

    let mut commands = vec![vec!["cargo".to_string(), "test".to_string()]];
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

pub fn gate_tests(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    config: &Config,
) -> GateResult {
    let timeout = config
        .int("tests", "timeout_secs", 900, Some(&target.name))
        .max(0) as u64;
    let commands = test_commands(config, target, repo);
    let contract = format!(
        "{} [tests] {}",
        config.source(),
        commands
            .iter()
            .map(|argv| argv.join(" "))
            .collect::<Vec<_>>()
            .join(" && ")
    );

    let mut details: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut status = PASS;

    for argv in &commands {
        let verdict = run_command(runner, repo, target, argv, timeout);
        details.extend(verdict.details);
        problems.extend(verdict.problems);
        if verdict.failed {
            status = FAIL;
        }
    }

    let summary = if details.is_empty() {
        "no test command".to_string()
    } else {
        details.join("; ")
    };
    details.extend(problems);

    GateResult::new("tests", status, summary, details)
        .contract(contract)
        .fixes(fix_hints("tests").iter().copied())
}

struct CommandVerdict {
    details: Vec<String>,
    problems: Vec<String>,
    failed: bool,
}

fn run_command(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    argv: &[String],
    timeout: u64,
) -> CommandVerdict {
    let command = argv.join(" ");
    let result = process::dev(runner, &target.dir(repo), argv, Some(timeout));
    let output = result.combined();
    let reported = summaries(&output);

    if reported.is_empty() {
        return CommandVerdict {
            details: tail_details(&output),
            problems: vec![format!(
                "`{command}` printed no test summary (exit {})",
                result.code
            )],
            failed: true,
        };
    }

    let passed: i64 = reported.iter().map(|(_, passed, _)| passed).sum();
    let failed: i64 = reported.iter().map(|(_, _, failed)| failed).sum();
    let broken = failed > 0 || !result.ok();
    let mut problems = Vec::new();
    if broken {
        problems.push(format!(
            "`{command}`: {failed} failed{}",
            failed_names(&output)
        ));
    }

    CommandVerdict {
        details: vec![format!("{command}: {passed} passed, {failed} failed")],
        problems,
        failed: broken,
    }
}

/// Every `test result:` line, as `(status, passed, failed)`.
fn summaries(output: &str) -> Vec<(String, i64, i64)> {
    summary_pattern()
        .captures_iter(output)
        .map(|captures| {
            (
                captures[1].to_string(),
                captures[2].parse().unwrap_or_default(),
                captures[3].parse().unwrap_or_default(),
            )
        })
        .collect()
}

fn failed_names(output: &str) -> String {
    let mut names: Vec<String> = failure_pattern()
        .captures_iter(output)
        .map(|captures| captures[1].to_string())
        .collect();
    names.sort();
    names.dedup();

    if names.is_empty() {
        return String::new();
    }
    format!(
        " -> {}",
        names
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn tail_details(output: &str) -> Vec<String> {
    let lines: Vec<&str> = output.trim().lines().collect();
    let start = lines.len().saturating_sub(10);
    lines[start..]
        .iter()
        .map(|line| format!("  {}", line.trim()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo(config: Option<&str>) -> MiniRepo {
        MiniRepo::build(config)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root).expect("config loads")
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
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
            test_commands(&config_for(&repo), &Target::workspace_target(), &repo.root),
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
        let frontend = Target::crate_target("frontend", false);

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
            test_commands(&config, &Target::crate_target("desktop", false), &repo.root),
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
                &Target::crate_target("frontend", false),
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
                &Target::crate_target("desktop", false),
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
                &Target::crate_target("frontend", false),
                &repo.root
            ),
            vec![argv(&["cargo", "test"])]
        );
    }

    #[test]
    fn failing_tests_are_named_in_the_tests_gate() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\", \"--all-features\"]
        ",
        ));
        let output = "test thing::works ... ok\ntest thing::breaks ... FAILED\n\ntest result: FAILED. 1 passed; 1 failed\n";
        let runner = FakeRunner::with(&[("cargo test", 101, output)]);

        let result = gate_tests(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
        );

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("thing::breaks"));
        assert!(result.contract.contains("cargo test --all-features"));
    }

    #[test]
    fn a_green_suite_reports_its_counts() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let output = "test result: ok. 41 passed; 0 failed; 0 ignored\n";
        let runner = FakeRunner::with(&[("cargo test", 0, output)]);

        let result = gate_tests(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
        );

        assert_eq!(result.status, PASS);
        assert_eq!(result.summary, "cargo test: 41 passed, 0 failed");
    }

    #[test]
    fn tests_gate_fails_when_the_command_prints_no_summary() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let runner = FakeRunner::with(&[("cargo test", 127, "cargo: command not found")]);

        let result = gate_tests(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
        );

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("no test summary"));
    }

    #[test]
    fn a_nonzero_exit_with_every_test_green_is_still_a_failure() {
        let repo = repo(Some(
            "
            version = 1

            [tests]
            command = [\"cargo\", \"test\"]
        ",
        ));
        let runner =
            FakeRunner::with(&[("cargo test", 101, "test result: ok. 3 passed; 0 failed\n")]);

        let result = gate_tests(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
        );

        assert_eq!(result.status, FAIL);
        assert!(result.details.join(" ").contains("`cargo test`: 0 failed"));
    }
}
