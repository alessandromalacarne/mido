//! Resolving a run: its revision, its files, and the targets it measures.

use super::{report_path, scratch_dir, Session};
use crate::cli::Args;
use crate::config::Config;
use crate::error::GuardrailsError;
use crate::gate::{Gate, GATES};
use crate::lang::Lang;
use crate::process::{self, Runner};
use crate::targets::Target;
use std::collections::BTreeMap;
use std::path::Path;

pub fn build_session(
    args: &Args,
    repo: &Path,
    config: Config,
    runner: &dyn Runner,
    lang: Lang,
) -> Result<Session, GuardrailsError> {
    Ok(Session {
        repo: repo.to_path_buf(),
        config,
        lang,
        files: process::all_files(runner, repo),
        revision: process::git(runner, repo, &["rev-parse", "HEAD"])
            .trim()
            .to_string(),
        dirty: process::dirty_hash(runner, repo),
        gates: selected_gates(args),
        scratch: scratch_dir(),
        baseline_lcov: args.baseline_lcov.clone(),
        report_path: report_path(args.report.clone(), repo),
        apply_aid: args.apply_workspace_aid,
        as_json: args.json,
    })
}

/// The gates to run: what `--gate` selected, or the whole ladder.
fn selected_gates(args: &Args) -> Vec<Gate> {
    if args.gates.is_empty() {
        GATES.to_vec()
    } else {
        args.gates.clone()
    }
}

/// Which targets this run measures: the whole workspace, or the packages the
/// `-p` flags name, each resolved the way cargo resolves `--package`.
pub fn select_targets(
    args: &Args,
    repo: &Path,
    config: &Config,
    lang: Lang,
    targets: &BTreeMap<String, Target>,
) -> Result<Vec<Target>, GuardrailsError> {
    if args.packages.is_empty() {
        return Ok(vec![workspace_roll_up(targets)?]);
    }

    let mut selected: Vec<Target> = Vec::new();
    for spec in &args.packages {
        let target = lang.resolve_package(repo, config, spec)?;
        if !selected.iter().any(|known| known.name == target.name) {
            selected.push(target);
        }
    }
    Ok(selected)
}

/// The default target of a bare run: the workspace roll-up.
fn workspace_roll_up(targets: &BTreeMap<String, Target>) -> Result<Target, GuardrailsError> {
    targets
        .values()
        .find(|target| target.path.is_empty())
        .cloned()
        .ok_or_else(|| {
            GuardrailsError::setup("this repository has no workspace target to measure")
                .hint("run --list-targets to see what can be measured")
        })
}
