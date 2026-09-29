//! Gate 3 — maintainability metrics.

use crate::config::Config;
use crate::gates::{fix_hints, general};
use crate::metrics::{units_from_document, Unit};
use crate::process::{self, Runner};
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;
use std::path::Path;

pub fn analysis_tool(config: &Config, target: &Target) -> String {
    config
        .command("analysis", "tool", target, "rust-code-analysis")
        .split('|')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn analysis_units(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    files: &[String],
    on_error: &mut Vec<String>,
) -> Vec<Unit> {
    let mut units = Vec::new();

    for path in files {
        let args: Vec<String> = [
            "rust-code-analysis-cli",
            "-m",
            "--pr",
            "-O",
            "json",
            "-p",
            path,
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
        let result = process::dev(runner, repo, &target.dir(repo), &args, None);
        if !result.ok() || result.stdout.trim().is_empty() {
            on_error.push(format!(
                "{path}: rust-code-analysis could not read it (exit {})",
                result.code
            ));
            continue;
        }

        match serde_json::from_str(&result.stdout) {
            Ok(document) => units.extend(units_from_document(path, &document)),
            Err(error) => on_error.push(format!(
                "{path}: rust-code-analysis printed no usable json ({error})"
            )),
        }
    }
    units
}

pub fn gate_analysis(
    target: &Target,
    config: &Config,
    units: &[Unit],
    tool_errors: &[String],
) -> GateResult {
    let mi_min = config.float("analysis", "mi_min", 20.0, Some(&target.name));
    let cognitive_max = config.float("analysis", "cognitive_max", 15.0, Some(&target.name));
    let tool = analysis_tool(config, target);
    let contract = format!(
        "{} [analysis] mi_min={}, cognitive_max={} (tool {tool})",
        config.source(),
        general(mi_min),
        general(cognitive_max)
    );

    if let Some(result) = unsupported_tool(config, target, &tool, &contract) {
        return result;
    }

    let Some((worst_mi, worst_cognitive)) = worst_units(units) else {
        let detail = tool_errors
            .first()
            .cloned()
            .unwrap_or_else(|| "no function metrics to judge".to_string());
        return GateResult::new("analysis", INCOMPLETE, detail, tool_errors.to_vec())
            .contract(contract)
            .fixes(fix_hints("analysis").iter().copied());
    };

    let problems = judge_units(units, mi_min, cognitive_max);
    let mut details = vec![
        worst_mi_line(worst_mi, mi_min),
        worst_cognitive_line(worst_cognitive, cognitive_max),
    ];
    let status = if problems.is_empty() { PASS } else { FAIL };
    details.extend(problems);

    GateResult::new(
        "analysis",
        status,
        format!(
            "worst MI {:.1}, cognitive {}",
            worst_mi.mi, worst_cognitive.cognitive
        ),
        details,
    )
    .contract(contract)
    .fixes(fix_hints("analysis").iter().copied())
}

/// A `tool` the config names but this runner cannot drive is INCOMPLETE, never a pass.
fn unsupported_tool(
    config: &Config,
    target: &Target,
    tool: &str,
    contract: &str,
) -> Option<GateResult> {
    let named = config
        .section("analysis", Some(&target.name))
        .contains_key("tool");
    if !named || tool == "rust-code-analysis" {
        return None;
    }

    Some(
        GateResult::new(
            "analysis",
            INCOMPLETE,
            format!("tool `{tool}` is not supported by this runner"),
            [format!(
                "`.guardrails.toml` names `{tool}`; this script only drives rust-code-analysis-cli"
            )],
        )
        .contract(contract)
        .fixes(["point [analysis] tool at rust-code-analysis, or extend the runner with a parser for that tool"]),
    )
}

fn worst_units(units: &[Unit]) -> Option<(&Unit, &Unit)> {
    let worst_mi = units
        .iter()
        .min_by(|left, right| left.mi.total_cmp(&right.mi))?;
    let worst_cognitive = units.iter().max_by_key(|unit| unit.cognitive)?;
    Some((worst_mi, worst_cognitive))
}

fn worst_mi_line(unit: &Unit, mi_min: f64) -> String {
    format!(
        "worst MI: {:.1} (min {}) — {} @ {}",
        unit.mi,
        general(mi_min),
        unit.name,
        unit.path
    )
}

fn worst_cognitive_line(unit: &Unit, cognitive_max: f64) -> String {
    format!(
        "worst cognitive: {} (max {}) — {} @ {}",
        unit.cognitive,
        general(cognitive_max),
        unit.name,
        unit.path
    )
}

fn judge_units(units: &[Unit], mi_min: f64, cognitive_max: f64) -> Vec<String> {
    let mut problems = Vec::new();
    for unit in units {
        if unit.mi < mi_min {
            problems.push(format!(
                "{}: {} MI {:.1} (min {})",
                unit.path,
                unit.name,
                unit.mi,
                general(mi_min)
            ));
        }
        if unit.cognitive as f64 > cognitive_max {
            problems.push(format!(
                "{}: {} cognitive complexity {} (max {})",
                unit.path,
                unit.name,
                unit.cognitive,
                general(cognitive_max)
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeRunner, MiniRepo};

    fn repo(config: Option<&str>) -> MiniRepo {
        MiniRepo::build(config)
    }

    fn unit(name: &str, mi: f64, cognitive: i64) -> Unit {
        Unit {
            mi,
            cognitive,
            ..Unit::new("src/foo.rs", name)
        }
    }

    fn config_for(repo: &MiniRepo) -> Config {
        Config::load(&repo.root).expect("config loads")
    }

    #[test]
    fn function_metrics_are_read_from_the_tool() {
        let repo = repo(None);
        let document = serde_json::json!({
            "spaces": [{ "name": "parse", "kind": "function", "metrics": { "loc": { "sloc": 9 }, "mi": { "mi_original": 40.0 } } }]
        });
        let runner = FakeRunner::with(&[("rust-code-analysis-cli", 0, &document.to_string())]);
        let mut errors = Vec::new();

        let units = analysis_units(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &mut errors,
        );

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].name, "parse");
        assert!(errors.is_empty());
    }

    #[test]
    fn a_file_the_tool_cannot_read_is_an_error() {
        let repo = repo(None);
        let runner = FakeRunner::with(&[("rust-code-analysis-cli", 1, "")]);
        let mut errors = Vec::new();

        let units = analysis_units(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &mut errors,
        );

        assert!(units.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("could not read it"));
    }

    #[test]
    fn unparsable_json_is_an_error() {
        let repo = repo(None);
        let runner = FakeRunner::with(&[("rust-code-analysis-cli", 0, "{oops")]);
        let mut errors = Vec::new();

        analysis_units(
            &runner,
            &repo.root,
            &Target::workspace_target(),
            &["src/foo.rs".to_string()],
            &mut errors,
        );

        assert!(errors.iter().any(|error| error.contains("no usable json")));
    }

    #[test]
    fn the_worst_units_are_judged_and_named() {
        let repo = repo(None);
        let units = vec![unit("good", 80.0, 2), unit("hard", 12.0, 30)];

        let result = gate_analysis(&Target::workspace_target(), &config_for(&repo), &units, &[]);

        assert_eq!(result.status, FAIL);
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("worst MI: 12.0 (min 20) — hard")));
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("worst cognitive: 30 (max 15)")));
        assert!(result
            .details
            .iter()
            .any(|line| line.contains("src/foo.rs: hard MI 12.0 (min 20)")));
    }

    #[test]
    fn healthy_units_pass_with_their_numbers_reported() {
        let repo = repo(None);
        let units = vec![unit("good", 80.0, 2)];

        let result = gate_analysis(&Target::workspace_target(), &config_for(&repo), &units, &[]);

        assert_eq!(result.status, PASS);
        assert_eq!(result.summary, "worst MI 80.0, cognitive 2");
        assert!(result.contract.contains("mi_min=20, cognitive_max=15"));
    }

    #[test]
    fn no_function_metrics_is_incomplete_never_a_pass() {
        let repo = repo(None);

        let result = gate_analysis(&Target::workspace_target(), &config_for(&repo), &[], &[]);

        assert_eq!(result.status, INCOMPLETE);
        assert_eq!(result.summary, "no function metrics to judge");
    }

    #[test]
    fn the_first_tool_error_explains_the_incompleteness() {
        let repo = repo(None);
        let errors = vec!["src/foo.rs: rust-code-analysis could not read it (exit 1)".to_string()];

        let result = gate_analysis(
            &Target::workspace_target(),
            &config_for(&repo),
            &[],
            &errors,
        );

        assert_eq!(result.status, INCOMPLETE);
        assert_eq!(result.summary, errors[0]);
        assert!(result.details.contains(&errors[0]));
    }

    #[test]
    fn an_unsupported_analysis_tool_is_incomplete_with_a_pointer() {
        let repo = repo(Some(
            "
            version = 1

            [analysis]
            tool = \"lizard\"
        ",
        ));

        let result = gate_analysis(&Target::workspace_target(), &config_for(&repo), &[], &[]);

        assert_eq!(result.status, INCOMPLETE);
        assert!(result.summary.contains("`lizard` is not supported"));
        assert!(result.fixes[0].contains("point [analysis] tool at rust-code-analysis"));
    }

    #[test]
    fn a_piped_tool_setting_keeps_only_the_first_name() {
        let repo = repo(Some(
            "
            version = 1

            [analysis]
            tool = \"rust-code-analysis | something-else\"
        ",
        ));

        assert_eq!(
            analysis_tool(&config_for(&repo), &Target::workspace_target()),
            "rust-code-analysis"
        );
    }

    #[test]
    fn thresholds_print_without_a_trailing_zero() {
        assert_eq!(general(20.0), "20");
        assert_eq!(general(0.0), "0");
        assert_eq!(general(12.5), "12.5");
    }
}
