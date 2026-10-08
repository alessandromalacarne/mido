//! What a run is scoped to: the workspace, or one package of it.

use crate::config::{value, Config};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    /// The cargo package name its manifest declares; `None` for the roll-up
    /// and for declared targets that are not crates.
    pub package: Option<String>,
    pub path: String,
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

    pub fn workspace_target(manifest: &str) -> Self {
        Self {
            name: "workspace".to_string(),
            package: None,
            path: String::new(),
            manifest: Some(manifest.to_string()),
            workspace_member: true,
        }
    }

    pub fn crate_target(crate_name: &str, member: bool, manifest: &str) -> Self {
        Self {
            name: crate_name.to_string(),
            package: None,
            path: crate_name.to_string(),
            manifest: Some(format!("{crate_name}/{manifest}")),
            workspace_member: member,
        }
    }

    pub fn with_package(mut self, package: Option<String>) -> Self {
        self.package = package;
        self
    }
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
        package: None,
        manifest: Some(declared_manifest(section, &path, manifest)),
        workspace_member: path.is_empty() || members.contains(&path),
        path,
    }
}

fn declared_manifest(section: &toml::Table, path: &str, manifest: &str) -> String {
    section
        .get("manifest")
        .and_then(value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| manifest_for(path, manifest))
}

pub fn manifest_for(path: &str, manifest: &str) -> String {
    if path.is_empty() {
        manifest.to_string()
    } else {
        format!("{path}/{manifest}")
    }
}

/// The repo files a target measures, as the target's own tools spell them: the
/// whole target, except other people's crates — a standalone crate's files
/// belong to it, not to the workspace roll-up.
pub fn target_files(
    files: &[String],
    target: &Target,
    targets: &BTreeMap<String, Target>,
) -> Vec<String> {
    let excluded: Vec<&str> = if target.path.is_empty() {
        targets
            .values()
            .filter(|other| !other.workspace_member)
            .map(|other| other.path.as_str())
            .filter(|path| !path.is_empty() && !path.contains('/'))
            .collect()
    } else {
        Vec::new()
    };

    files
        .iter()
        .filter(|path| under_dir(path, &target.path))
        .filter(|path| !excluded.iter().any(|dir| under_dir(path, dir)))
        .map(|path| strip_dir(path, &target.path))
        .collect()
}

fn under_dir(path: &str, dir: &str) -> bool {
    dir.is_empty()
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn strip_dir(path: &str, dir: &str) -> String {
    if dir.is_empty() {
        return path.to_string();
    }
    match path.strip_prefix(dir) {
        Some(rest) => rest.trim_start_matches('/').to_string(),
        None => path.to_string(),
    }
}

#[cfg(test)]
mod tests;
