//! cargo targets: workspace members, excluded crates, and package resolution.

use crate::config::{value, Config};
use crate::error::GuardrailsError;
use crate::lang::rust::MANIFEST;
use crate::targets::{declared_targets, Target};
use std::collections::BTreeMap;
use std::path::Path;

/// `(members, excluded)` from the root manifest; empty when there is none.
pub fn workspace_layout(repo: &Path) -> (Vec<String>, Vec<String>) {
    let manifest = repo.join(MANIFEST);
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
            .and_then(value::string_array)
            .unwrap_or_default()
    };
    (listed("members"), listed("exclude"))
}

/// Top-level directories holding a manifest.
pub fn crate_dirs(repo: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(repo) else {
        return Vec::new();
    };

    let mut crates: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_dir() && path.join(MANIFEST).exists()
        })
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !name.starts_with('.'))
        .collect();
    crates.sort();
    crates
}

/// The package name a crate's manifest declares, as cargo's `-p` spells it.
pub fn package_name(repo: &Path, dir: &str) -> Option<String> {
    let text = std::fs::read_to_string(repo.join(dir).join(MANIFEST)).ok()?;
    let data = text.parse::<toml::Table>().ok()?;
    data.get("package")
        .and_then(value::as_table)
        .and_then(|package| package.get("name"))
        .and_then(value::as_str)
        .map(str::to_string)
}

pub fn detect_targets(repo: &Path, config: &Config) -> BTreeMap<String, Target> {
    let (members, excluded) = workspace_layout(repo);
    let mut targets = BTreeMap::new();

    targets.insert(
        "workspace".to_string(),
        Target {
            name: "workspace".to_string(),
            package: None,
            path: String::new(),
            manifest: if repo.join(MANIFEST).exists() {
                Some(MANIFEST.to_string())
            } else {
                None
            },
            workspace_member: true,
        },
    );

    for crate_name in crate_dirs(repo) {
        if members.contains(&crate_name) {
            targets.entry(crate_name.clone()).or_insert_with(|| {
                Target::crate_target(&crate_name, true, MANIFEST)
                    .with_package(package_name(repo, &crate_name))
            });
        } else if excluded.contains(&crate_name) {
            targets.insert(
                crate_name.clone(),
                Target::crate_target(&crate_name, false, MANIFEST)
                    .with_package(package_name(repo, &crate_name)),
            );
        }
    }

    targets.extend(declared_targets(config, &members, MANIFEST));
    targets
}

/// Resolve `-p` the way cargo resolves `--package`: by package name, with the
/// listed target names accepted for declared targets. A name no target answers
/// to is a setup error — never a silent whole-workspace run.
pub fn resolve_package(
    repo: &Path,
    config: &Config,
    spec: &str,
) -> Result<Target, GuardrailsError> {
    let targets = detect_targets(repo, config);
    if let Some(target) = targets.get(spec) {
        return Ok(target.clone());
    }
    if let Some(target) = targets
        .values()
        .find(|target| target.package.as_deref() == Some(spec))
    {
        return Ok(target.clone());
    }

    Err(unknown_package_error(spec, &targets))
}

fn unknown_package_error(spec: &str, targets: &BTreeMap<String, Target>) -> GuardrailsError {
    let mut known: Vec<String> = targets
        .values()
        .map(|target| match &target.package {
            Some(package) if *package != target.name => format!("{} ({package})", target.name),
            _ => target.name.clone(),
        })
        .collect();
    known.sort();

    GuardrailsError::setup(format!("package `{spec}` not found in this workspace"))
        .detail(format!("known names: {}", known.join(", ")))
        .hint("pass a package name, or run without -p to measure the whole workspace")
}

