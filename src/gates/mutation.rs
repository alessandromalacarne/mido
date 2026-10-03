//! Gate 6 — do the tests actually detect broken code?

use crate::config::Config;
use crate::gates::{fix_hints, GateRun};
use crate::lang::{Lang, MutationScope, MutationSummary};
use crate::metrics::percent;
use crate::process::last_lines;
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Scope;

pub fn gate_mutation(runner: &dyn Runner, run: &GateRun<'_>) -> GateResult {
    let GateRun {
        repo,
        target,
        config,
        lang,
        scope,
        changed,
        ..
    } = *run;
    let minimum = config.float("mutation", "kill_rate_min", 70.0, Some(&target.name));
    let timeout = config.int("mutation", "timeout_secs", 3600, Some(&target.name));
    let configured = config.text_setting("mutation", "scope", "changed", Some(&target.name));
    let mutable: Vec<String> = changed
        .iter()
        .filter(|path| lang.is_source(path))
        .cloned()
        .collect();

    // A run pointed at paths has no diff to patch, so the files themselves are
    // the scope. Nothing mutable is nothing to measure — never a pass.
    if scope == Scope::Paths && mutable.is_empty() {
        return nothing_mutable(config, lang);
    }

    let args = match mutation_command(runner, run, lang, &configured, &mutable) {
        Ok(args) => args,
        Err(error) => return unpatched(error),
    };
    let contract = format!(
        "{} [mutation] kill_rate_min={minimum}, scope={configured} via `{}`",
        config.source(),
        args.join(" ")
    );

    let result = process::dev(
        runner,
        lang.env_tool(),
        &target.dir(repo),
        &args,
        Some(timeout.max(0) as u64),
    );
    judge_mutation(&result, &contract, minimum, timeout, lang)
}

/// The mutation command: the module's command plus its scope flags and the
/// per-mutant timeout.
fn mutation_command(
    runner: &dyn Runner,
    run: &GateRun<'_>,
    lang: Lang,
    configured: &str,
    mutable: &[String],
) -> Result<Vec<String>, std::io::Error> {
    let GateRun {
        repo,
        target,
        config,
        scope,
        scratch,
        ..
    } = *run;
    let mut args = config
        .argv("mutation", "command", target)
        .or_else(|| lang.fallback_argv("mutation", "command"))
        .unwrap_or_default();
    let patch = scratch.join(format!("guardrails-changed-{}.patch", target.name));
    let scoped = MutationScope {
        configured,
        scope,
        changed: mutable,
        patch: &patch,
    };
    args.extend(lang.mutation_scope_args(runner, repo, target, scoped)?);

    args.push("--timeout".to_string());
    args.push(lang.mutation_timeout().to_string());
    Ok(args)
}

/// The verdict for a patch that could not be written: INCOMPLETE, never a pass.
fn unpatched(error: std::io::Error) -> GateResult {
    GateResult::new(
        "mutation",
        INCOMPLETE,
        "the changed-file patch could not be written",
        [error.to_string()],
    )
    .fixes(fix_hints("mutation").iter().copied())
}

/// The verdict for a path scope with nothing the mutation tool can mutate.
fn nothing_mutable(config: &Config, lang: Lang) -> GateResult {
    let label = lang.source_label();
    let tool = lang.mutation_tool();
    GateResult::new(
        "mutation",
        INCOMPLETE,
        format!("no {label} file in the paths given — nothing to mutate"),
        [format!(
            "{tool} mutates {label} files; the paths given hold none"
        )],
    )
    .contract(format!(
        "{} [mutation] scope=explicit paths, no {label} file to mutate",
        config.source()
    ))
    .fixes(fix_hints("mutation").iter().copied())
}

fn judge_mutation(
    result: &process::Outcome,
    contract: &str,
    minimum: f64,
    timeout: i64,
    lang: Lang,
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

    let Some(MutationSummary {
        total,
        caught,
        missed,
        unviable,
    }) = lang.mutation_summary(&output)
    else {
        return GateResult::new(
            "mutation",
            INCOMPLETE,
            format!("{} produced no summary", lang.mutation_tool()),
            tail(&output),
        )
        .contract(contract)
        .fixes(fix_hints("mutation").iter().copied());
    };

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
        format!("{rate:.1}% killed (min {minimum})"),
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
    use crate::lang::Lang;
    use crate::targets::Target;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo() -> MiniRepo {
        MiniRepo::build(None)
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root, &Lang::Rust).expect("config loads")
    }

    fn gate_run_for<'a>(
        repo: &'a MiniRepo,
        config: &'a Config,
        changed: &'a [String],
    ) -> GateRun<'a> {
        let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
        crate::test_support::gate_run(&repo.root, target, config, changed)
    }

    const SUMMARY: &str = "120 mutants tested in 3m: 10 missed, 105 caught, 5 unviable\n";

    #[test]
    fn a_summary_that_omits_the_zero_categories_is_read() {
        let repo = repo();
        let output = "115 mutants tested in 6m: 96 caught, 19 unviable\n";
        let runner = FakeRunner::with(&[("cargo mutants", 0, output)]);

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

        assert_eq!(result.status, PASS);
        assert!(result.summary.contains("100.0% killed (min 70)"));
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("96 caught, 0 missed, 19 unviable")));
    }

    #[test]
    fn a_kill_rate_above_the_minimum_passes_and_reports_the_numbers() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

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

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

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

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

        assert_eq!(result.status, INCOMPLETE);
        assert!(result.summary.contains("timed out after 3600s"));
    }

    #[test]
    fn output_without_a_summary_is_incomplete() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, "cargo-mutants: nothing to do")]);

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

        assert_eq!(result.status, INCOMPLETE);
        assert_eq!(result.summary, "cargo-mutants produced no summary");
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

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

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

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

        assert!(result.contract.contains("--in-place"));
        assert!(!result.contract.contains("--in-diff"));
        assert!(
            !result.contract.contains("--file"),
            "`all` scopes nothing: no path may ride along: {}",
            result.contract
        );
    }

    #[test]
    fn the_mutant_timeout_is_passed_through() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let result = gate_mutation(&runner, &gate_run_for(&repo, &config_for(&repo), &[]));

        assert!(result.contract.contains("--timeout 120"));
    }

    #[test]
    fn explicit_paths_scope_the_mutants_to_the_named_files() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);
        let changed = vec!["src/foo.rs".to_string(), "README.md".to_string()];
        let config = config_for(&repo);
        let mut run = gate_run_for(&repo, &config, &changed);
        run.scope = Scope::Paths;
        let result = gate_mutation(&runner, &run);

        assert!(
            result.contract.contains("--file src/foo.rs"),
            "{}",
            result.contract
        );
        assert!(
            !result.contract.contains("--in-diff"),
            "{}",
            result.contract
        );
        assert!(
            !result.contract.contains("README.md"),
            "{}",
            result.contract
        );
    }

    #[test]
    fn explicit_paths_without_a_rust_file_never_run_the_mutants() {
        let repo = repo();
        let runner = FakeRunner::with(&[("cargo mutants", 0, SUMMARY)]);

        let changed = vec!["README.md".to_string()];
        let config = config_for(&repo);
        let mut run = gate_run_for(&repo, &config, &changed);
        run.scope = Scope::Paths;
        let result = gate_mutation(&runner, &run);

        assert_eq!(result.status, INCOMPLETE);
        assert!(result.summary.contains("no rust file in the paths given"));
        assert!(!runner.called_with("cargo mutants"));
    }
}
