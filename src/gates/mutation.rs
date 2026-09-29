//! Gate 6 — do the tests actually detect broken code?

use crate::config::Config;
use crate::gates::{fix_hints, general, RUST_EXT};
use crate::metrics::percent;
use crate::process::last_lines;
use crate::process::{self, Command, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::{covers, Target};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

pub const DEFAULT_MUTANT_TIMEOUT: i64 = 120;

fn summary_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(\d+) mutants tested in [^:]+: (\d+) missed, (\d+) caught, (\d+) unviable")
            .expect("valid pattern")
    })
}

fn git_args(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// The patch cargo-mutants should mutate: the working-tree diff, untracked files included.
fn changed_scope_args(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    patch: &Path,
) -> Result<Vec<String>, std::io::Error> {
    let untracked: Vec<String> = process::git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .lines()
    .filter(|path| path.ends_with(RUST_EXT) && covers(path, target))
    .map(str::to_string)
    .collect();

    if !untracked.is_empty() {
        let mut add = vec!["add".to_string(), "-N".to_string()];
        add.extend(untracked.iter().cloned());
        process::git(runner, repo, &git_args(&add));
    }

    let mut diff = vec!["git".to_string(), "diff".to_string()];
    if !target.path.is_empty() {
        diff.push(format!("--relative={}", target.path));
    }
    let result = runner.exec(&Command::new(repo, diff));
    std::fs::write(patch, &result.stdout)?;

    if !untracked.is_empty() {
        // Leave the index as it was found: intent-to-add entries would otherwise
        // show up as staged in whatever commit comes next.
        let mut reset = vec!["reset".to_string(), "-q".to_string(), "--".to_string()];
        reset.extend(untracked);
        process::git(runner, repo, &git_args(&reset));
    }

    Ok(vec![
        "--in-place".to_string(),
        "--in-diff".to_string(),
        patch.to_string_lossy().to_string(),
    ])
}

pub fn gate_mutation(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    config: &Config,
    scratch: &Path,
) -> GateResult {
    let minimum = config.float("mutation", "kill_rate_min", 70.0, Some(&target.name));
    let timeout = config.int("mutation", "timeout_secs", 3600, Some(&target.name));
    let scope = config.text_setting("mutation", "scope", "changed", Some(&target.name));
    let command = config.command("mutation", "command", target, "cargo mutants");
    let patch = scratch.join(format!("guardrails-changed-{}.patch", target.name));

    let mut args = process::split(&command);
    match scope_args(runner, repo, target, &scope, &patch) {
        Ok(scoped) => args.extend(scoped),
        Err(error) => {
            return GateResult::new(
                "mutation",
                INCOMPLETE,
                "the changed-file patch could not be written",
                [error.to_string()],
            )
            .fixes(fix_hints("mutation").iter().copied());
        }
    }

    args.push("--timeout".to_string());
    args.push(DEFAULT_MUTANT_TIMEOUT.to_string());
    let contract = format!(
        "{} [mutation] kill_rate_min={}, scope={scope} via `{}`",
        config.source(),
        general(minimum),
        args.join(" ")
    );

    let result = process::dev(
        runner,
        repo,
        &target.dir(repo),
        &args,
        Some(timeout.max(0) as u64),
    );
    judge_mutation(&result, &contract, minimum, timeout)
}

/// `cargo mutants` in place, either against the changed-file patch or a named path.
fn scope_args(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    scope: &str,
    patch: &Path,
) -> Result<Vec<String>, std::io::Error> {
    if scope == "changed" {
        return changed_scope_args(runner, repo, target, patch);
    }
    if scope.is_empty() || scope == "all" {
        return Ok(vec!["--in-place".to_string()]);
    }
    Ok(vec![
        "--in-place".to_string(),
        "--file".to_string(),
        scope.to_string(),
    ])
}

fn judge_mutation(
    result: &process::Outcome,
    contract: &str,
    minimum: f64,
    timeout: i64,
) -> GateResult {
    let output = result.combined();

    if result.code == process::TIMEOUT_EXIT {
        return GateResult::new(
            "mutation",
            INCOMPLETE,
            format!("timed out after {timeout}s"),
            tail(&output),
        )
        .contract(contract)
        .fixes(fix_hints("mutation").iter().copied());
    }

    let Some(summary) = summary_pattern().captures(&output) else {
        return GateResult::new(
            "mutation",
            INCOMPLETE,
            "cargo-mutants produced no summary",
            tail(&output),
        )
        .contract(contract)
        .fixes(fix_hints("mutation").iter().copied());
    };

    let total: i64 = summary[1].parse().unwrap_or_default();
    let missed: i64 = summary[2].parse().unwrap_or_default();
    let caught: i64 = summary[3].parse().unwrap_or_default();
    let unviable: i64 = summary[4].parse().unwrap_or_default();
    let rate = percent(caught, caught + missed);

    let mut details = vec![format!(
        "{total} mutants: {caught} caught, {missed} missed, {unviable} unviable -> {rate:.1}% killed"
    )];
    details.extend(
        output
            .lines()
            .filter(|line| line.starts_with("MISSED"))
            .map(|line| line.trim().to_string()),
    );

    let status = if rate < minimum { FAIL } else { PASS };
    GateResult::new(
        "mutation",
        status,
        format!("{rate:.1}% killed (min {})", general(minimum)),
        details,
    )
    .contract(contract)
    .fixes(fix_hints("mutation").iter().copied())
}

fn tail(output: &str) -> Vec<String> {
    last_lines(output, 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root).expect("config loads")
    }

    fn scratch(repo: &MiniRepo) -> std::path::PathBuf {
        let scratch = repo.root.join("scratch");
        std::fs::create_dir_all(&scratch).expect("scratch");
        scratch
    }

    const SUMMARY: &str = "120 mutants tested in 3m: 10 missed, 105 caught, 5 unviable\n";

    #[test]
    fn a_kill_rate_above_the_minimum_passes_and_reports_the_numbers() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert_eq!(result.status, PASS);
        assert!(result.summary.contains("91.3% killed (min 70)"));
        assert!(result.details[0].contains("120 mutants: 105 caught, 10 missed, 5 unviable"));
    }

    #[test]
    fn a_kill_rate_below_the_minimum_fails_and_lists_the_survivors() {
        let repo = repo();
        let output =
            "100 mutants tested in 3m: 60 missed, 40 caught, 0 unviable\nMISSED  src/foo.rs:12:5 replace + with - in parse\n";
        let runner = FakeRunner::with(&[("cargo mutants", 0, output)]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert_eq!(result.status, FAIL);
        assert!(result.summary.contains("40.0% killed (min 70)"));
        assert!(result
            .details
            .iter()
            .any(|line| line.starts_with("MISSED  src/foo.rs")));
    }

    #[test]
    fn an_endless_run_times_out_as_incomplete() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 124, "still going")]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert_eq!(result.status, INCOMPLETE);
        assert!(result.summary.contains("timed out after 3600s"));
    }

    #[test]
    fn output_without_a_summary_is_incomplete() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, "cargo-mutants: nothing to do")]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert_eq!(result.status, INCOMPLETE);
        assert_eq!(result.summary, "cargo-mutants produced no summary");
    }

    #[test]
    fn the_changed_scope_writes_the_working_tree_patch_first() {
        let repo = repo();
        let runner = FakeRunner::with(&[("git diff", 0, "diff --git a/src/foo.rs b/src/foo.rs\n")]);
        let scratch = scratch(&repo);

        let args = changed_scope_args(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &scratch.join("patch"),
        )
        .expect("patch written");

        assert!(args[0] == "--in-place" && args[1] == "--in-diff");
        let patch = std::fs::read_to_string(scratch.join("patch")).expect("patch");
        assert!(patch.contains("diff --git"));
    }

    #[test]
    fn untracked_rust_files_are_added_to_the_index_and_then_released() {
        let repo = repo();
        let runner = FakeRunner {
            responses: vec![
                (
                    "ls-files".to_string(),
                    crate::process::Outcome::new(0, "frontend/src/new.rs\nREADME.md\n", ""),
                ),
                (
                    "git diff".to_string(),
                    crate::process::Outcome::new(0, "diff\n", ""),
                ),
            ],
            ..FakeRunner::default()
        };

        changed_scope_args(
            &runner,
            &repo.root,
            &Target::crate_target("frontend", false),
            &scratch(&repo).join("patch"),
        )
        .expect("patch written");

        assert!(runner.called_with("add -N frontend/src/new.rs"));
        assert!(!runner.called_with("add -N README.md"));
        assert!(runner.called_with("reset -q -- frontend/src/new.rs"));
        assert!(runner.called_with("--relative=frontend"));
    }

    #[test]
    fn the_scope_setting_can_name_a_path_instead_of_the_diff() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [mutation]
            scope = \"src/parser.rs\"
        ",
        ));
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert!(result.contract.contains("--file src/parser.rs"));
        assert!(result.contract.contains("scope=src/parser.rs"));
    }

    #[test]
    fn the_all_scope_mutates_in_place_without_a_patch() {
        let repo = MiniRepo::build(Some(
            "
            version = 1

            [mutation]
            scope = \"all\"
        ",
        ));
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert!(result.contract.contains("--in-place"));
        assert!(!result.contract.contains("--in-diff"));
    }

    #[test]
    fn the_mutant_timeout_is_passed_through() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let result = gate_mutation(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &config_for(&repo),
            &scratch(&repo),
        );

        assert!(result.contract.contains("--timeout 120"));
    }
}
