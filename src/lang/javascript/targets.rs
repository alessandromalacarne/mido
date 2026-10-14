//! package.json targets: workspace members matched by pattern, standalone
//! packages, and package resolution.

use crate::config::Config;
use crate::error::GuardrailsError;
use crate::lang::javascript::MANIFEST;
use crate::targets::{declared_targets, resolve_named, Target};
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The `workspaces` patterns the root manifest declares — the array form or
/// the `{ "packages": [...] }` object form. Negated (`!`) entries are kept:
/// they take matches away.
pub fn workspace_patterns(repo: &Path) -> Vec<String> {
    let Some(root) = read_manifest(&repo.join(MANIFEST)) else {
        return Vec::new();
    };
    let Some(workspaces) = root.get("workspaces") else {
        return Vec::new();
    };

    let listed = workspaces
        .as_array()
        .or_else(|| workspaces.get("packages").and_then(Json::as_array));
    listed.map(|items| strings(items)).unwrap_or_default()
}

fn strings(items: &[Json]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

/// The package name a package.json declares, as `-p` spells it.
pub fn package_name(repo: &Path, dir: &str) -> Option<String> {
    let path = if dir.is_empty() {
        repo.join(MANIFEST)
    } else {
        repo.join(dir).join(MANIFEST)
    };
    read_manifest(&path)?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

fn read_manifest(path: &Path) -> Option<Json> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// The workspace layout: `(members, excluded)` — the directories the patterns
/// match, and the ones a negated pattern took back out. Excluded packages are
/// still packages: they become their own targets, outside the roll-up.
pub fn workspace_layout(repo: &Path, patterns: &[String]) -> (Vec<String>, Vec<String>) {
    let includes: Vec<&str> = patterns
        .iter()
        .map(String::as_str)
        .filter(|pattern| !pattern.starts_with('!'))
        .collect();
    let excludes: Vec<&str> = patterns
        .iter()
        .map(String::as_str)
        .filter_map(|pattern| pattern.strip_prefix('!'))
        .collect();

    let mut members: BTreeSet<String> = BTreeSet::new();
    let mut excluded: BTreeSet<String> = BTreeSet::new();
    for pattern in includes {
        for dir in candidate_dirs(repo, pattern) {
            if !repo.join(&dir).join(MANIFEST).exists() {
                continue;
            }
            if excludes
                .iter()
                .any(|exclude| matches_pattern(exclude, &dir))
            {
                excluded.insert(dir);
            } else {
                members.insert(dir);
            }
        }
    }
    (
        members.into_iter().collect(),
        excluded.into_iter().collect(),
    )
}

/// Every directory the pattern could match, within its depth.
fn candidate_dirs(repo: &Path, pattern: &str) -> Vec<String> {
    let segments = pattern_segments(pattern);
    let max_depth = if segments.contains(&"**") {
        usize::MAX
    } else {
        segments.len()
    };

    let mut dirs = Vec::new();
    walk_dirs(repo, "", 0, max_depth, &mut dirs);
    dirs.retain(|dir| matches_pattern(pattern, dir));
    dirs
}

/// Walk the tree, skipping what is never a package: hidden directories and
/// node_modules.
fn walk_dirs(root: &Path, rel: &str, depth: usize, max_depth: usize, out: &mut Vec<String>) {
    if depth >= max_depth {
        return;
    }
    let directory = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let child = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        out.push(child.clone());
        walk_dirs(root, &child, depth + 1, max_depth, out);
    }
}

fn pattern_segments(pattern: &str) -> Vec<&str> {
    pattern
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect()
}

/// Whether a relative directory matches one workspace pattern: `*` matches a
/// single segment, `**` any number of them.
pub fn matches_pattern(pattern: &str, dir: &str) -> bool {
    let pattern = pattern_segments(pattern);
    let dir: Vec<&str> = dir
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    match_segments(&pattern, &dir)
}

fn match_segments(pattern: &[&str], dir: &[&str]) -> bool {
    match pattern.split_first() {
        None => dir.is_empty(),
        Some((&"**", rest)) => {
            match_segments(rest, dir) || (!dir.is_empty() && match_segments(pattern, &dir[1..]))
        }
        Some((segment, rest)) => match dir.split_first() {
            Some((first, dir_rest)) => {
                (*segment == "*" || segment == first) && match_segments(rest, dir_rest)
            }
            None => false,
        },
    }
}

/// Top-level directories holding a manifest: packages outside the workspace
/// patterns become their own targets.
fn top_level_package_dirs(repo: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(repo) else {
        return Vec::new();
    };
    let mut dirs: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_dir() && path.join(MANIFEST).exists()
        })
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !name.starts_with('.') && name != "node_modules")
        .collect();
    dirs.sort();
    dirs
}

pub fn detect_targets(repo: &Path, config: &Config) -> BTreeMap<String, Target> {
    let (members, excluded) = workspace_layout(repo, &workspace_patterns(repo));
    let mut targets = BTreeMap::new();

    targets.insert(
        "workspace".to_string(),
        Target {
            name: "workspace".to_string(),
            package: None,
            path: String::new(),
            manifest: repo.join(MANIFEST).exists().then(|| MANIFEST.to_string()),
            workspace_member: true,
        },
    );

    for dir in &members {
        targets.insert(
            dir.clone(),
            Target::package_target(dir, true, MANIFEST).with_package(package_name(repo, dir)),
        );
    }

    for dir in excluded.iter().chain(top_level_package_dirs(repo).iter()) {
        if members.contains(dir) {
            continue;
        }
        targets.insert(
            dir.clone(),
            Target::package_target(dir, false, MANIFEST).with_package(package_name(repo, dir)),
        );
    }

    targets.extend(declared_targets(config, &members, MANIFEST));
    targets
}

/// Resolve `-p` the way the manifest spells the package: by the name it
/// declares, with the listed target names accepted for declared targets.
pub fn resolve_package(
    repo: &Path,
    config: &Config,
    spec: &str,
) -> Result<Target, GuardrailsError> {
    resolve_named(&detect_targets(repo, config), spec)
}

#[cfg(test)]
mod tests;
