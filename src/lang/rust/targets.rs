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
    fn excluded_crates_become_their_own_targets() {
        let repo = repo(None);

        let detected = targets(&repo);

        assert_eq!(detected.get("frontend").expect("frontend").path, "frontend");
        assert!(!detected.get("frontend").expect("frontend").workspace_member);
        assert_eq!(
            detected
                .get("frontend")
                .expect("frontend")
                .package
                .as_deref(),
            Some("frontend")
        );
        assert!(detected.contains_key("desktop"));
    }

    #[test]
    fn member_targets_carry_their_package_names() {
        let repo = repo(None);

        let detected = targets(&repo);

        assert_eq!(
            detected.get("cli").expect("cli").package.as_deref(),
            Some("cli")
        );
        assert_eq!(
            detected.get("api").expect("api").package.as_deref(),
            Some("api")
        );
    }

    #[test]
    fn a_package_name_that_differs_from_the_directory_still_resolves() {
        let repo = repo(None);
        std::fs::write(
            repo.root.join("cli/Cargo.toml"),
            "[package]\nname = \"cli-tool\"\n",
        )
        .expect("manifest");
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let target = resolve_package(&repo.root, &config, "cli-tool").expect("resolves");

        assert_eq!(target.name, "cli");
        assert_eq!(target.path, "cli");
    }

    #[test]
    fn explicit_target_section_extends_the_detected_set() {
        let repo = repo(Some(
            "
            version = 1

            [targets.tui]
            path = \"cli/src/tui\"
        ",
        ));

        assert_eq!(targets(&repo).get("tui").expect("tui").path, "cli/src/tui");
    }

    #[test]
    fn a_target_section_defaults_its_path_to_its_name() {
        let repo = repo(Some(
            "
            version = 1

            [targets.api]
        ",
        ));

        assert_eq!(targets(&repo).get("api").expect("api").path, "api");
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
    fn package_resolution_accepts_a_declared_name() {
        let repo = repo(Some(
            "
            version = 1

            [targets.tui]
            path = \"cli\"
        ",
        ));
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let target = resolve_package(&repo.root, &config, "tui").expect("resolves");

        assert_eq!(target.path, "cli");
    }

    #[test]
    fn an_unknown_package_is_a_setup_error_listing_the_known_names() {
        let repo = repo(None);
        let config = Config::load(&repo.root, &Lang::Rust).expect("config loads");

        let message = resolve_package(&repo.root, &config, "nope")
            .expect_err("unknown package")
            .render();

        assert!(message.contains("package `nope` not found"), "{message}");
        assert!(message.contains("frontend"), "{message}");
        assert!(message.contains("workspace"), "{message}");
    }

    #[test]
    fn workspace_layout_reads_members_and_excludes() {
        let repo = repo(None);

        let (members, excluded) = workspace_layout(&repo.root);

        assert_eq!(members, vec!["cli", "api", "lib"]);
        assert_eq!(excluded, vec!["frontend", "desktop"]);
    }

    #[test]
    fn targets_without_a_workspace_manifest_have_no_manifest() {
        let repo = repo(Some("version = 1\n"));
        std::fs::remove_file(repo.root.join("Cargo.toml")).expect("no root manifest");

        let detected = targets(&repo);
        let workspace = detected.get("workspace").expect("workspace target");

        assert_eq!(workspace.manifest, None);
        assert_eq!(workspace.package, None);
    }
}
