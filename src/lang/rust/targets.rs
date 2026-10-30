//! cargo targets: workspace members, excluded crates, and package resolution.

use crate::config::{value, Config};
use crate::error::GuardrailsError;
use crate::lang::rust::MANIFEST;
use crate::targets::{declared_targets, resolve_named, Target};
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
                Target::package_target(&crate_name, true, MANIFEST)
                    .with_package(package_name(repo, &crate_name))
            });
        } else if excluded.contains(&crate_name) {
            targets.insert(
                crate_name.clone(),
                Target::package_target(&crate_name, false, MANIFEST)
                    .with_package(package_name(repo, &crate_name)),
            );
        }
    }

    targets.extend(declared_targets(config, &members, MANIFEST));
    targets
}

/// Resolve `-p` the way cargo resolves `--package`: by package name, with the
/// listed target names accepted for declared targets.
pub fn resolve_package(
    repo: &Path,
    config: &Config,
    spec: &str,
) -> Result<Target, GuardrailsError> {
    resolve_named(&detect_targets(repo, config), spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::test_support::MiniRepo;

    fn targets(repo: &MiniRepo) -> BTreeMap<String, Target> {
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");
        detect_targets(&repo.root, &config)
    }

    fn repo(config: Option<&str>) -> MiniRepo {
        MiniRepo::build(config)
    }

    #[test]
    fn workspace_members_collapse_into_one_target() {
        let repo = repo(None);

        let detected = targets(&repo);
        let workspace = detected.get("workspace").expect("workspace target");

        assert_eq!(workspace.path, "");
        assert_eq!(workspace.package, None);
        assert!(workspace.workspace_member);
    }

    #[test]
    fn a_declared_target_names_the_manifest_under_its_path() {
        let repo = repo(Some(
            "
            version = 1

            [targets.tui]
            path = \"cli\"
        ",
        ));

        assert_eq!(
            targets(&repo).get("tui").expect("tui").manifest.as_deref(),
            Some("cli/Cargo.toml")
        );
    }
}
