//! `--path`: the files a run was pointed at, instead of a diff.

use super::{canonical_candidate, Target};
use crate::config::Config;
use crate::error::GuardrailsError;
use crate::lang::Lang;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The files the run was pointed at: a target name resolves to its directory,
/// a folder is walked, a file is taken as it is. No diff is consulted.
pub fn explicit_paths(
    repo: &Path,
    config: &Config,
    specs: &[PathBuf],
    lang: Lang,
) -> Result<Vec<String>, GuardrailsError> {
    let root = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    let targets = lang.detect_targets(&root, config);
    let mut files: Vec<PathBuf> = Vec::new();

    for spec in specs {
        let named = targets.get(spec.to_string_lossy().as_ref());
        let path = match named {
            Some(target) => target.dir(&root),
            None => canonical_candidate(&root, spec),
        };
        if !path.exists() {
            return Err(unknown_path_error(&root, spec, &targets));
        }
        if path.is_dir() {
            walk_files(&path, &mut files);
        } else {
            files.push(path);
        }
    }

    relative_files(&root, files)
}

/// The walked files as repo-relative paths, deduped and in a stable order.
fn relative_files(root: &Path, files: Vec<PathBuf>) -> Result<Vec<String>, GuardrailsError> {
    let mut relative: Vec<String> = Vec::new();
    for file in files {
        let file = std::fs::canonicalize(&file).unwrap_or(file);
        let Ok(stripped) = file.strip_prefix(root) else {
            return Err(outside_repo_error(&file, root));
        };
        relative.push(stripped.to_string_lossy().replace('\\', "/"));
    }
    relative.sort();
    relative.dedup();
    Ok(relative)
}

/// Every file under `directory`, in a stable order; hidden entries are skipped.
fn walk_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !is_hidden(path))
        .collect();
    children.sort();

    for child in children {
        if child.is_dir() {
            walk_files(&child, files);
        } else {
            files.push(child);
        }
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

fn unknown_path_error(
    repo: &Path,
    spec: &Path,
    targets: &BTreeMap<String, Target>,
) -> GuardrailsError {
    GuardrailsError::setup(format!("`{}` does not exist", spec.display()))
        .detail(format!("looked under {}", repo.display()))
        .detail(format!(
            "known targets: {}",
            targets.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
        .hint("pass a file, a folder or a target name (--list-targets)")
}

fn outside_repo_error(file: &Path, repo: &Path) -> GuardrailsError {
    GuardrailsError::setup(format!("`{}` is outside the repo", file.display()))
        .detail(format!("--path measures files under {}", repo.display()))
}
