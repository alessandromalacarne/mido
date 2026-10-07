//! What a run is scoped to. `frontend` is one target among several, not a special case.

mod paths;

pub use paths::explicit_paths;

use crate::config::{value, Config};
use crate::error::GuardrailsError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub path: String,
    pub scope: Vec<String>,
    pub manifest: Option<String>,
    pub workspace_member: bool,
}

impl Target {
    pub fn dir(&self, repo: &Path) -> PathBuf {
        if self.path.is_empty() {
            repo.to_path_buf()
        } else {
            repo.join(&self.path)
        }
    }

    pub fn label(&self) -> String {
        let path = if self.path.is_empty() {
            "."
        } else {
            &self.path
        };
        format!("{} ({path}/)", self.name)
    }

    pub fn strip(&self, path: &str) -> String {
        if !self.path.is_empty() {
            if let Some(rest) = path.strip_prefix(&format!("{}/", self.path)) {
                return rest.to_string();
            }
        }
        path.to_string()
    }

    pub fn workspace_target(manifest: &str) -> Self {
        Self {
            name: "workspace".to_string(),
            path: String::new(),
            scope: Vec::new(),
            manifest: Some(manifest.to_string()),
            workspace_member: true,
        }
    }

    pub fn crate_target(crate_name: &str, member: bool, manifest: &str) -> Self {
        Self {
            name: crate_name.to_string(),
            path: crate_name.to_string(),
            scope: vec![format!("{crate_name}/")],
            manifest: Some(format!("{crate_name}/{manifest}")),
            workspace_member: member,
        }
    }
}

/// Where the paths a run measures came from: the diff, the `--path` given, or
/// the whole target (no diff read).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    Diff,
    Paths,
    Whole,
}

/// `[targets.<name>]` sections: the repo's own way to name a case. The module
/// supplies the manifest file its language uses.
pub fn declared_targets(
    config: &Config,
    members: &[String],
    manifest: &str,
) -> BTreeMap<String, Target> {
    let Some(targets) = config.data().get("targets").and_then(value::as_table) else {
        return BTreeMap::new();
    };

    targets
        .iter()
        .filter_map(|(name, value)| {
            value.as_table().map(|section| {
                (
                    name.clone(),
                    declared_target(name, section, members, manifest),
                )
            })
        })
        .collect()
}

fn declared_target(
    name: &str,
    section: &toml::Table,
    members: &[String],
    manifest: &str,
) -> Target {
    let path = section
        .get("path")
        .and_then(value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| name.to_string());
    let path = path.trim_matches('/').to_string();

    Target {
        name: name.to_string(),
        scope: declared_scope(section, &path),
        manifest: Some(declared_manifest(section, &path, manifest)),
        workspace_member: path.is_empty() || members.contains(&path),
        path,
    }
}

fn declared_scope(section: &toml::Table, path: &str) -> Vec<String> {
    match section
        .get("scope")
        .and_then(value::string_array)
        .filter(|entries| !entries.is_empty())
    {
        Some(entries) => entries.into_iter().map(with_trailing_slash).collect(),
        None => vec![directory_scope(path)],
    }
}

fn declared_manifest(section: &toml::Table, path: &str, manifest: &str) -> String {
    section
        .get("manifest")
        .and_then(value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| manifest_for(path, manifest))
}

fn with_trailing_slash(entry: String) -> String {
    if entry.ends_with('/') || entry.is_empty() {
        entry
    } else {
        format!("{entry}/")
    }
}

pub fn directory_scope(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

pub fn manifest_for(path: &str, manifest: &str) -> String {
    if path.is_empty() {
        manifest.to_string()
    } else {
        format!("{path}/{manifest}")
    }
}

pub(crate) fn canonical_candidate(repo: &Path, spec: &Path) -> PathBuf {
    let requested = if spec.is_absolute() {
        spec.to_path_buf()
    } else {
        repo.join(spec)
    };
    std::fs::canonicalize(&requested).unwrap_or(requested)
}

pub(crate) fn repo_relative(repo: &Path, candidate: &Path) -> String {
    match candidate.strip_prefix(repo) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => candidate.to_string_lossy().to_string(),
    }
}

pub(crate) fn top_level(path: &str) -> String {
    path.split('/').next().unwrap_or_default().to_string()
}

pub fn covers(path: &str, target: &Target) -> bool {
    if target.scope.is_empty() {
        return true;
    }
    target
        .scope
        .iter()
        .any(|prefix| prefix.is_empty() || path.starts_with(prefix.as_str()))
}

/// Changed files that belong to the target, as paths the target's tools print.
pub fn scope_changed(changed: &[String], target: &Target) -> Vec<String> {
    changed
        .iter()
        .filter(|path| covers(path, target))
        .map(|path| target.strip(path))
        .collect()
}

/// The narrowest target that covers every changed file a target owns.
///
/// `None` means no cargo target owns any changed file (docs-only change, or a
/// change to the tooling itself) — nothing to measure. Changes spread over
/// several targets are not guessed at: measuring half of a diff silently is
/// how a gate ends up not being a gate.
pub fn pick_auto_target(
    targets: &BTreeMap<String, Target>,
    changed: &[String],
) -> Result<Option<Target>, GuardrailsError> {
    let owned: Vec<&String> = changed
        .iter()
        .filter(|path| targets.values().any(|target| covers(path, target)))
        .collect();
    if owned.is_empty() {
        return Ok(None);
    }

    let mut covering: Vec<&Target> = targets
        .values()
        .filter(|target| owned.iter().all(|path| covers(path, target)))
        .collect();
    if covering.is_empty() {
        return Err(ambiguous_targets_error(targets, &owned));
    }

    covering.sort_by_key(|target| {
        (
            target.path.is_empty(),
            target.scope.len(),
            target.path.clone(),
        )
    });
    Ok(covering.first().map(|target| (*target).clone()))
}

/// Which target owns which of the changed files, for the human reading the error.
fn ambiguous_targets_error(
    targets: &BTreeMap<String, Target>,
    owned: &[&String],
) -> GuardrailsError {
    let mut owners: Vec<String> = targets
        .values()
        .filter(|target| owned.iter().any(|path| covers(path, target)))
        .map(|target| owner_line(target, owned))
        .collect();
    owners.sort();

    GuardrailsError::setup("the changed files span more than one target")
        .details(owners)
        .hint("name the targets explicitly, or run with --all")
}

fn owner_line(target: &Target, owned: &[&String]) -> String {
    let paths: Vec<String> = owned
        .iter()
        .filter(|path| covers(path, target))
        .map(|path| (*path).clone())
        .collect();
    let shown = paths.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    let more = if paths.len() > 3 { " …" } else { "" };
    format!("{}: {shown}{more}", target.name)
}

#[cfg(test)]
mod tests;
