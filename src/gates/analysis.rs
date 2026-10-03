//! Gate 3 — maintainability metrics.

use crate::config::Config;
use crate::gates::{fix_hints, GateRun};
use crate::lang::Lang;
use crate::metrics::Unit;
use crate::report::{GateResult, FAIL, INCOMPLETE, PASS};
use crate::targets::Target;

pub fn analysis_tool(config: &Config, target: &Target, lang: Lang) -> String {
    config.text_setting(
        "analysis",
        "tool",
        lang.analysis_supported_tool(),
        Some(&target.name),
    )
}

pub fn gate_analysis(run: &GateRun<'_>, units: &[Unit], tool_errors: &[String]) -> GateResult {
    let GateRun {
        target,
        config,
        lang,
        ..
    } = *run;
    let mi_min = config.float("analysis", "mi_min", 20.0, Some(&target.name));
    let cognitive_max = config.float("analysis", "cognitive_max", 15.0, Some(&target.name));
    let tool = analysis_tool(config, target, lang);
    let contract = format!(
        "{} [analysis] mi_min={}, cognitive_max={} (tool {tool})",
        config.source(),
        mi_min,
        cognitive_max
    );

    if let Some(result) = unsupported_tool(config, target, &tool, &contract, lang) {
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

/// A `tool` the config names but this module cannot drive is INCOMPLETE, never a pass.
fn unsupported_tool(
    config: &Config,
    target: &Target,
    tool: &str,
    contract: &str,
    lang: Lang,
) -> Option<GateResult> {
    let supported = lang.analysis_supported_tool();
    let named = config
        .section("analysis", Some(&target.name))
        .contains_key("tool");
    if !named || tool == supported {
        return None;
    }

    Some(
        GateResult::new(
            "analysis",
            INCOMPLETE,
            format!("tool `{tool}` is not supported by this runner"),
            [format!(
                "`.mido.toml` names `{tool}`; this module only drives {supported}-cli"
            )],
        )
        .contract(contract)
        .fixes([format!(
            "point [analysis] tool at {supported}, or extend the module with a parser for that tool"
        )]),
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
        unit.mi, mi_min, unit.name, unit.path
    )
}

fn worst_cognitive_line(unit: &Unit, cognitive_max: f64) -> String {
    format!(
        "worst cognitive: {} (max {}) — {} @ {}",
        unit.cognitive, cognitive_max, unit.name, unit.path
    )
}

fn judge_units(units: &[Unit], mi_min: f64, cognitive_max: f64) -> Vec<String> {
    let mut problems = Vec::new();
    for unit in units {
        if unit.mi < mi_min {
            problems.push(format!(
                "{}: {} MI {:.1} (min {})",
                unit.path, unit.name, unit.mi, mi_min
            ));
        }
        if unit.cognitive as f64 > cognitive_max {
            problems.push(format!(
                "{}: {} cognitive complexity {} (max {})",
                unit.path, unit.name, unit.cognitive, cognitive_max
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::test_support::MiniRepo;

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
        Config::load(&repo.root, &Lang::Rust).expect("config loads")
    }

    fn gate_run_for<'a>(repo: &'a MiniRepo, config: &'a Config) -> GateRun<'a> {
        let target = Box::leak(Box::new(Target::workspace_target("Cargo.toml")));
        crate::test_support::gate_run(&repo.root, target, config, &[])
    }

    #[test]
    fn the_worst_units_are_judged_and_named() {
        let repo = repo(None);
        let units = vec![unit("good", 80.0, 2), unit("hard", 12.0, 30)];

        let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo)), &units, &[]);

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

        let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo)), &units, &[]);

        assert_eq!(result.status, PASS);
        assert_eq!(result.summary, "worst MI 80.0, cognitive 2");
        assert!(result.contract.contains("mi_min=20, cognitive_max=15"));
    }

    #[test]
    fn no_function_metrics_is_incomplete_never_a_pass() {
        let repo = repo(None);

        let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo)), &[], &[]);

        assert_eq!(result.status, INCOMPLETE);
        assert_eq!(result.summary, "no function metrics to judge");
    }

    #[test]
    fn the_first_tool_error_explains_the_incompleteness() {
        let repo = repo(None);
        let errors = vec!["src/foo.rs: rust-code-analysis could not read it (exit 1)".to_string()];

        let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo)), &[], &errors);

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

        let result = gate_analysis(&gate_run_for(&repo, &config_for(&repo)), &[], &[]);

        assert_eq!(result.status, INCOMPLETE);
        assert!(result.summary.contains("`lizard` is not supported"));
        assert!(result.fixes[0].contains("point [analysis] tool at rust-code-analysis"));
    }
}
