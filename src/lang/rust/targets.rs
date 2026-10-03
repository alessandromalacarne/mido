//! cargo targets: workspace members, excluded crates, and path resolution.

use crate::config::{value, Config};
use crate::error::GuardrailsError;
use crate::lang::rust::MANIFEST;
use crate::targets::{
    canonical_candidate, declared_targets, directory_scope, manifest_for, repo_relative, top_level,
    Target,
};
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

pub fn detect_targets(repo: &Path, config: &Config) -> BTreeMap<String, Target> {
    let (members, excluded) = workspace_layout(repo);
    let mut targets = BTreeMap::new();

    targets.insert(
        "workspace".to_string(),
        Target {
            name: "workspace".to_string(),
            path: String::new(),
            scope: members.iter().map(|member| format!("{member}/")).collect(),
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
            targets
                .entry(crate_name.clone())
                .or_insert_with(|| Target::crate_target(&crate_name, true, MANIFEST));
        } else if excluded.contains(&crate_name) {
            targets.insert(
                crate_name.clone(),
                Target::crate_target(&crate_name, false, MANIFEST),
            );
        }
    }

    targets.extend(declared_targets(config, &members, MANIFEST));
    targets
}

pub fn resolve_target(repo: &Path, config: &Config, spec: &str) -> Result<Target, GuardrailsError> {
    let targets = detect_targets(repo, config);
    if let Some(target) = targets.get(spec) {
        return Ok(target.clone());
    }

    let candidate = canonical_candidate(repo, Path::new(spec));
    if !candidate.exists() {
        return Err(unknown_target_error(repo, spec, &targets));
    }
    if !candidate.join(MANIFEST).exists() {
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
    let is_root = relative.is_empty();
    let path = if is_root { String::new() } else { relative };
    Ok(Target {
        workspace_member: is_root || members.contains(&top_level(&path)),
        scope: vec![directory_scope(&path)],
        manifest: Some(manifest_for(&path, MANIFEST)),
        name,
        path,
    })
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
        .hint(format!(
            "pass one of the known target names, a directory holding a {MANIFEST}, or --list-targets"
        ))
}

fn no_manifest_error(spec: &str, candidate: &Path) -> GuardrailsError {
    GuardrailsError::setup(format!("`{spec}` holds no {MANIFEST}"))
        .detail(format!("looked for {}", candidate.join(MANIFEST).display()))
        .hint("the ladder measures a cargo target; point it at a crate directory")
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
        assert_eq!(workspace.scope, vec!["cli/", "api/", "lib/"]);
        assert!(workspace.workspace_member);
    }

    #[test]
    fn excluded_crates_become_their_own_targets() {
        let repo = repo(None);

        let detected = targets(&repo);

        assert_eq!(
            detected.get("frontend").expect("frontend").scope,
            vec!["frontend/"]
        );
        assert!(!detected.get("frontend").expect("frontend").workspace_member);
        assert!(detected.contains_key("desktop"));
    }

    #[test]
    fn explicit_target_section_extends_the_detected_set() {
        let repo = repo(Some(
            "
            version = 1

            [targets.tui]
            path = \"cli\"
            scope = [\"cli/src/tui/\"]
        ",
        ));

        assert_eq!(
            targets(&repo).get("tui").expect("tui").scope,
            vec!["cli/src/tui/"]
        );
    }

    #[test]
    fn a_target_section_defaults_its_scope_to_its_path() {
        let repo = repo(Some(
            "
            version = 1

            [targets.api]
            path = \"api\"
        ",
        ));

        assert_eq!(targets(&repo).get("api").expect("api").scope, vec!["api/"]);
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

    #[test]
    fn resolving_the_repo_root_marks_it_a_workspace_member() {
        let repo = repo(None);
        let root = std::fs::canonicalize(&repo.root).expect("canonical root");
        let config = Config::load(&root, &Lang::Rust).expect("config loads");

        let target = resolve_target(&root, &config, &root.to_string_lossy())
            .expect("the repo root resolves");

        assert!(target.workspace_member);
        assert_eq!(target.path, "");
    }

    #[test]
    fn target_parameter_resolves_by_name() {
        let repo = repo(None);
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let target = resolve_target(&repo.root, &config, "frontend").expect("resolves");

        assert_eq!(target.name, "frontend");
        assert_eq!(target.path, "frontend");
    }

    #[test]
    fn target_parameter_resolves_by_path() {
        let repo = repo(None);
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let target = resolve_target(
            &repo.root,
            &config,
            &repo.root.join("frontend").to_string_lossy(),
        )
        .expect("resolves");

        assert_eq!(target.name, "frontend");
        assert_eq!(target.dir(&repo.root), repo.root.join("frontend"));
    }

    #[test]
    fn a_path_outside_the_repo_resolves_to_its_own_target() {
        let repo = repo(None);
        let elsewhere = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            elsewhere.path().join("Cargo.toml"),
            "[package]\nname = \"outer\"\n",
        )
        .expect("manifest");
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let target = resolve_target(&repo.root, &config, &elsewhere.path().to_string_lossy())
            .expect("resolves");

        assert_eq!(target.path, elsewhere.path().to_string_lossy());
        assert!(!target.workspace_member);
    }

    #[test]
    fn unknown_target_is_a_setup_error_listing_the_known_names() {
        let repo = repo(None);
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let message = resolve_target(&repo.root, &config, "nope")
            .expect_err("unknown target")
            .render();

        assert!(message.contains("nope"));
        assert!(message.contains("frontend"));
        assert!(message.contains("workspace"));
    }

    #[test]
    fn path_without_a_manifest_is_a_setup_error() {
        let repo = repo(None);
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let message = resolve_target(&repo.root, &config, "scripts")
            .expect_err("no manifest")
            .render();

        assert!(message.contains("Cargo.toml"));
    }

    #[test]
    fn workspace_layout_reads_members_and_excludes() {
        let repo = repo(None);

        let (members, excluded) = workspace_layout(&repo.root);

        assert_eq!(members, vec!["cli", "api", "lib"]);
        assert_eq!(excluded, vec!["frontend", "desktop"]);
    }

    #[test]
    fn targets_without_a_workspace_manifest_have_no_scope() {
        let repo = repo(Some("version = 1\n"));
        std::fs::remove_file(repo.root.join("Cargo.toml")).expect("no root manifest");

        let detected = targets(&repo);
        let workspace = detected.get("workspace").expect("workspace target");

        assert!(workspace.scope.is_empty());
        assert_eq!(workspace.manifest, None);
    }
}
