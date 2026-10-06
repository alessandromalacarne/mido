//! The workspace aid — a nested worktree cannot resolve the outer cargo workspace.

use crate::error::GuardrailsError;
use crate::lang::rust::MANIFEST;
use crate::process::{self, Outcome, Runner};
use crate::targets::Target;
use std::io::Write;
use std::path::Path;

/// Ask cargo which manifest it takes as the workspace root from the target dir.
pub fn probe_workspace(runner: &dyn Runner, repo: &Path, target: &Target) -> Outcome {
    process::dev(
        runner,
        "cargo",
        &target.dir(repo),
        &[
            "cargo".to_string(),
            "locate-project".to_string(),
            "--workspace".to_string(),
            "--message-format".to_string(),
            "plain".to_string(),
        ],
        None,
    )
}

/// Whether cargo cannot resolve the crate's own workspace root from `target.dir()`.
///
/// Excluded crates are fine in the main checkout but not in a nested worktree:
/// the root manifest lists `members`, so cargo refuses the package (exit 101,
/// "believes it's in a workspace when it's not") or silently answers with the
/// outer workspace root. Both mean the local `[workspace]` aid is required.
pub fn workspace_aid_needed(probe: &Outcome, repo: &Path, target: &Target) -> bool {
    if target.workspace_member || target.path.is_empty() {
        return false;
    }
    if !probe.ok() {
        return true;
    }

    let located: Vec<&str> = probe
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let Some(last) = located.last() else {
        return true;
    };

    let answer =
        std::fs::canonicalize(last.trim()).unwrap_or_else(|_| Path::new(last.trim()).to_path_buf());
    let expected = repo.join(target.manifest.clone().unwrap_or_default());
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    answer != expected
}

/// The marker the aid writes, and checks for, so a second run is a no-op.
const AID_MARKER: &str = "# Local worktree aid (not for commit)";
const AID_HEADER: &str =
    "# Local worktree aid (not for commit): declares this package as its own\n\
                           # workspace root, so cargo does not walk up into the main checkout.\n\
                           [workspace]\n\n";

pub fn apply_workspace_aid(runner: &dyn Runner, repo: &Path, target: &Target, out: &mut dyn Write) {
    let manifest = target.dir(repo).join(MANIFEST);
    // The aid edits exactly one file: the target's own manifest, at an absolute
    // path, in a directory that already exists. Anything else means the caller is
    // confused, and a manifest is not the way to find out.
    let inside_a_directory = manifest
        .parent()
        .is_some_and(|directory| directory.is_absolute() && directory.is_dir());
    if !inside_a_directory {
        return;
    }

    if let Ok(text) = std::fs::read_to_string(&manifest) {
        if !text.starts_with(AID_MARKER) && !text.starts_with("[workspace]") {
            let updated = format!("{AID_HEADER}{text}");
            // Additive only: the aid never loses a line of the manifest it edits.
            if updated.ends_with(&text) {
                let _ = std::fs::write(&manifest, updated);
            }
        }
    }

    let manifest = target
        .manifest
        .clone()
        .unwrap_or_else(|| MANIFEST.to_string());
    let _ = process::git(
        runner,
        repo,
        &["update-index", "--skip-worktree", &manifest],
    );
    let _ = writeln!(
        out,
        "workspace aid applied: {manifest} declares its own [workspace] (skip-worktree)"
    );
}

pub fn validate_target_setup(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    apply_aid: bool,
    out: &mut dyn Write,
) -> Result<(), GuardrailsError> {
    ensure_manifest(repo, target)?;

    if target.workspace_member || target.path.is_empty() {
        return Ok(());
    }

    let probe = probe_workspace(runner, repo, target);
    if probe.code == process::NOT_FOUND_EXIT {
        return Err(cargo_missing_error(&probe));
    }
    if !workspace_aid_needed(&probe, repo, target) {
        return Ok(());
    }
    if apply_aid {
        apply_workspace_aid(runner, repo, target, out);
        return Ok(());
    }
    Err(aid_needed_error(target, &probe))
}

fn ensure_manifest(repo: &Path, target: &Target) -> Result<(), GuardrailsError> {
    let Some(manifest) = &target.manifest else {
        return Ok(());
    };
    let path = repo.join(manifest);
    if path.exists() {
        return Ok(());
    }

    Err(
        GuardrailsError::setup(format!("target `{}` has no manifest", target.name))
            .detail(format!("looked for {}", path.display()))
            .hint("pass --list-targets to see what can be measured"),
    )
}

fn cargo_missing_error(probe: &Outcome) -> GuardrailsError {
    let details = if probe.stderr.trim().is_empty() {
        vec!["`cargo locate-project` could not run".to_string()]
    } else {
        process::last_lines(&probe.stderr, 3)
    };

    GuardrailsError::setup("cargo is not available")
        .details(details)
        .hint("run the ladder inside `nix develop`, or install the Rust toolchain")
}

fn aid_needed_error(target: &Target, probe: &Outcome) -> GuardrailsError {
    let mut details = vec![
        "this happens in a worktree nested under .agents/, where cargo walks up into the main checkout instead"
            .to_string(),
    ];
    details.extend(probe.stderr.trim().lines().take(4).map(str::to_string));

    GuardrailsError::setup(format!(
        "cargo cannot resolve `{}`'s workspace root",
        target.path
    ))
    .details(details)
    .hint("re-run with --apply-workspace-aid to add the git-invisible [workspace] line")
}

#[cfg(test)]
mod tests;
