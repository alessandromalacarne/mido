//! What a run is scoped to. `frontend` is one target among several, not a special case.

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

    pub fn workspace_target() -> Self {
        Self {
            name: "workspace".to_string(),
            path: String::new(),
            scope: Vec::new(),
            manifest: Some("Cargo.toml".to_string()),
            workspace_member: true,
        }
    }

    pub fn crate_target(crate_name: &str, member: bool) -> Self {
        Self {
            name: crate_name.to_string(),
            path: crate_name.to_string(),
            scope: vec![format!("{crate_name}/")],
            manifest: Some(format!("{crate_name}/Cargo.toml")),
            workspace_member: member,
        }
    }
}

/// `(members, excluded)` from the root manifest; empty when there is none.
pub fn workspace_layout(repo: &Path) -> (Vec<String>, Vec<String>) {
    let manifest = repo.join("Cargo.toml");
    if !manifest.exists() {
        return (Vec::new(), Vec::new());
    }
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(data) = text.parse::<toml::Table>() else {
        return (Vec::new(), Vec::new());
    };

    let workspace = data.get("workspace").and_then(value::as_table);
    let listed = |key: &str| -> Vec<String> {
        workspace
            .and_then(|table| table.get(key))
            .and_then(value::string_list)
            .unwrap_or_default()
    };
    (listed("members"), listed("exclude"))
}

/// Top-level directories holding a `Cargo.toml`.
pub fn crate_dirs(repo: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(repo) else {
        return Vec::new();
    };

    let mut crates: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_dir() && path.join("Cargo.toml").exists()
        })
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !name.starts_with('.'))
        .collect();
    crates.sort();
    crates
}

/// `[targets.<name>]` sections: the repo's own way to name a case.
pub fn declared_targets(config: &Config, members: &[String]) -> BTreeMap<String, Target> {
    let Some(targets) = config.data().get("targets").and_then(value::as_table) else {
        return BTreeMap::new();
    };

    targets
        .iter()
        .filter_map(|(name, value)| {
            value
                .as_table()
                .map(|section| (name.clone(), declared_target(name, section, members)))
        })
        .collect()
}

fn declared_target(name: &str, section: &toml::Table, members: &[String]) -> Target {
    let path = section
        .get("path")
        .map(value::render)
        .unwrap_or_else(|| name.to_string());
    let path = path.trim_matches('/').to_string();

    Target {
        name: name.to_string(),
        scope: declared_scope(section, &path),
        manifest: Some(declared_manifest(section, &path)),
        workspace_member: path.is_empty() || members.contains(&path),
        path,
    }
}

fn declared_scope(section: &toml::Table, path: &str) -> Vec<String> {
    match section
        .get("scope")
        .and_then(value::string_list)
        .filter(|entries| !entries.is_empty())
    {
        Some(entries) => entries.into_iter().map(with_trailing_slash).collect(),
        None => vec![directory_scope(path)],
    }
}

fn declared_manifest(section: &toml::Table, path: &str) -> String {
    section
        .get("manifest")
        .and_then(value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| manifest_for(path))
}

fn with_trailing_slash(entry: String) -> String {
    if entry.ends_with('/') || entry.is_empty() {
        entry
    } else {
        format!("{entry}/")
    }
}

fn directory_scope(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

fn manifest_for(path: &str) -> String {
    if path.is_empty() {
        "Cargo.toml".to_string()
    } else {
        format!("{path}/Cargo.toml")
    }
}

pub fn detect_targets(repo: &Path, config: &Config) -> BTreeMap<String, Target> {
    let (members, excluded) = workspace_layout(repo);
    let mut targets = BTreeMap::new();

    targets.insert(
        "workspace".to_string(),
        Target {
            name: "workspace".to_string(),
            path: String::new(),
            scope: members.iter().map(|member| format!("{member}/")).collect(),
            manifest: if repo.join("Cargo.toml").exists() {
                Some("Cargo.toml".to_string())
            } else {
                None
            },
            workspace_member: true,
        },
    );

    for crate_name in crate_dirs(repo) {
        if members.contains(&crate_name) {
            targets
                .entry(crate_name.clone())
                .or_insert_with(|| Target::crate_target(&crate_name, true));
        } else if excluded.contains(&crate_name) {
            targets.insert(crate_name.clone(), Target::crate_target(&crate_name, false));
        }
    }

    targets.extend(declared_targets(config, &members));
    targets
}

pub fn resolve_target(repo: &Path, config: &Config, spec: &str) -> Result<Target, GuardrailsError> {
    let targets = detect_targets(repo, config);
    if let Some(target) = targets.get(spec) {
        return Ok(target.clone());
    }

    let candidate = canonical_candidate(repo, spec);
    if !candidate.exists() {
        return Err(unknown_target_error(repo, spec, &targets));
    }
    if !candidate.join("Cargo.toml").exists() {
        return Err(no_manifest_error(spec, &candidate));
    }

    let relative = repo_relative(repo, &candidate);
    let name = candidate
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| relative.clone());
    if let Some(target) = targets.get(&name) {
        return Ok(target.clone());
    }

    let (members, _) = workspace_layout(repo);
    let is_root = relative == ".";
    let path = if is_root { String::new() } else { relative };
    Ok(Target {
        workspace_member: is_root || members.contains(&top_level(&path)),
        scope: vec![directory_scope(&path)],
        manifest: Some(manifest_for(&path)),
        name,
        path,
    })
}

fn canonical_candidate(repo: &Path, spec: &str) -> PathBuf {
    let requested = Path::new(spec);
    let requested = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        repo.join(requested)
    };
    std::fs::canonicalize(&requested).unwrap_or(requested)
}

fn repo_relative(repo: &Path, candidate: &Path) -> String {
    match candidate.strip_prefix(repo) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => candidate.to_string_lossy().to_string(),
    }
}

fn top_level(path: &str) -> String {
    path.split('/').next().unwrap_or_default().to_string()
}

fn unknown_target_error(
    repo: &Path,
    spec: &str,
    targets: &BTreeMap<String, Target>,
) -> GuardrailsError {
    GuardrailsError::setup(format!("unknown target `{spec}`"))
        .detail(format!("no such name or path under {}", repo.display()))
        .detail(format!(
            "known targets: {}",
            targets.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
        .hint("pass one of the known target names, a directory holding a Cargo.toml, or --list-targets")
}

fn no_manifest_error(spec: &str, candidate: &Path) -> GuardrailsError {
    GuardrailsError::setup(format!("`{spec}` holds no Cargo.toml"))
        .detail(format!(
            "looked for {}",
            candidate.join("Cargo.toml").display()
        ))
        .hint("the ladder measures a cargo target; point it at a crate directory")
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
