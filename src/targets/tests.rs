use super::*;

fn crate_target(name: &str, member: bool) -> Target {
    Target::crate_target(name, member, "Cargo.toml")
}

fn targets(entries: &[(&str, bool)]) -> BTreeMap<String, Target> {
    entries
        .iter()
        .map(|(name, member)| (name.to_string(), crate_target(name, *member)))
        .collect()
}

fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

#[test]
fn a_manifest_path_joins_the_directory_and_the_file() {
    assert_eq!(manifest_for("", "Cargo.toml"), "Cargo.toml");
    assert_eq!(
        manifest_for("frontend", "Cargo.toml"),
        "frontend/Cargo.toml"
    );
    assert_eq!(
        manifest_for("frontend", "package.json"),
        "frontend/package.json"
    );
}

#[test]
fn labels_show_the_directory() {
    assert_eq!(
        Target::workspace_target("Cargo.toml").label(),
        "workspace (./)"
    );
    assert_eq!(
        Target::crate_target("frontend", false, "Cargo.toml").label(),
        "frontend (frontend/)"
    );
}

#[test]
fn a_package_target_measures_only_its_own_directory() {
    let all = targets(&[("lib", true), ("frontend", false)]);
    let listed = files(&[
        "lib/src/foo.rs",
        "lib/Cargo.toml",
        "frontend/src/main.rs",
        "README.md",
    ]);

    assert_eq!(
        target_files(&listed, &crate_target("lib", true), &all),
        files(&["src/foo.rs", "Cargo.toml"])
    );
}

#[test]
fn a_sibling_directory_with_a_shared_prefix_is_not_owned() {
    let all = targets(&[("frontend", false)]);
    let listed = files(&["frontend/src/main.rs", "frontend-old/src/main.rs"]);

    assert_eq!(
        target_files(&listed, &crate_target("frontend", false), &all),
        files(&["src/main.rs"])
    );
}

#[test]
fn the_workspace_measures_everything_but_standalone_crates() {
    let all = targets(&[("lib", true), ("frontend", false)]);
    let listed = files(&[
        "Cargo.toml",
        "README.md",
        "lib/src/foo.rs",
        "cli/src/main.rs",
        "frontend/src/main.rs",
        "frontend/Cargo.toml",
    ]);

    assert_eq!(
        target_files(&listed, &Target::workspace_target("Cargo.toml"), &all),
        files(&[
            "Cargo.toml",
            "README.md",
            "lib/src/foo.rs",
            "cli/src/main.rs",
        ])
    );
}

#[test]
fn a_workspace_member_is_never_excluded_from_the_roll_up() {
    let all = targets(&[("lib", true), ("api", true), ("frontend", false)]);
    let listed = files(&["lib/src/foo.rs", "api/src/bar.rs"]);

    assert_eq!(
        target_files(&listed, &Target::workspace_target("Cargo.toml"), &all),
        listed
    );
}

#[test]
fn a_declared_sub_directory_target_measures_its_subtree_and_stays_in_the_workspace() {
    let mut all = targets(&[("frontend", false)]);
    all.insert(
        "tui".to_string(),
        Target {
            name: "tui".to_string(),
            package: None,
            path: "cli/src/tui".to_string(),
            manifest: Some("cli/Cargo.toml".to_string()),
            workspace_member: false,
        },
    );
    let listed = files(&["cli/src/tui/app.rs", "cli/src/main.rs"]);

    assert_eq!(target_files(&listed, &all["tui"], &all), files(&["app.rs"]));
    assert_eq!(
        target_files(&listed, &Target::workspace_target("Cargo.toml"), &all),
        listed
    );
}

#[test]
fn a_stored_target_field_round_trips_through_the_file_list() {
    let target = crate_target("lib", true).with_package(Some("mido-guard".to_string()));

    assert_eq!(target.package.as_deref(), Some("mido-guard"));
    assert_eq!(target.dir(Path::new("/repo")), Path::new("/repo/lib"));
}
