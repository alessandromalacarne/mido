use super::*;
use crate::test_support::MiniRepo;

fn targets(repo: &MiniRepo) -> BTreeMap<String, Target> {
    let config = Config::load(&repo.root).expect("config loads");
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
fn target_parameter_resolves_by_name() {
    let repo = repo(None);
    let config = Config::load(&repo.root).expect("config loads");

    let target = resolve_target(&repo.root, &config, "frontend").expect("resolves");

    assert_eq!(target.name, "frontend");
    assert_eq!(target.path, "frontend");
}

#[test]
fn target_parameter_resolves_by_path() {
    let repo = repo(None);
    let config = Config::load(&repo.root).expect("config loads");

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
    let config = Config::load(&repo.root).expect("config loads");

    let target =
        resolve_target(&repo.root, &config, &elsewhere.path().to_string_lossy()).expect("resolves");

    assert_eq!(target.path, elsewhere.path().to_string_lossy());
    assert!(!target.workspace_member);
}

#[test]
fn unknown_target_is_a_setup_error_listing_the_known_names() {
    let repo = repo(None);
    let config = Config::load(&repo.root).expect("config loads");

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
    let config = Config::load(&repo.root).expect("config loads");

    let message = resolve_target(&repo.root, &config, "scripts")
        .expect_err("no manifest")
        .render();

    assert!(message.contains("Cargo.toml"));
}

#[test]
fn root_target_keeps_repo_relative_paths() {
    let target = Target {
        name: "workspace".to_string(),
        path: String::new(),
        scope: vec!["cli/".to_string(), "api/".to_string(), "lib/".to_string()],
        manifest: Some("Cargo.toml".to_string()),
        workspace_member: true,
    };
    let changed = vec!["lib/src/foo.rs".to_string(), "cli/src/main.rs".to_string()];

    assert_eq!(scope_changed(&changed, &target), changed);
}

#[test]
fn crate_target_strips_its_own_prefix() {
    let target = Target::crate_target("frontend", false);
    let changed = vec![
        "frontend/src/main.rs".to_string(),
        "frontend/tests/app.rs".to_string(),
    ];

    assert_eq!(
        scope_changed(&changed, &target),
        vec!["src/main.rs", "tests/app.rs"]
    );
}

#[test]
fn files_outside_the_target_are_dropped() {
    let target = Target::crate_target("frontend", false);
    let changed = vec![
        "frontend/src/main.rs".to_string(),
        "lib/src/foo.rs".to_string(),
        "src/main.rs".to_string(),
    ];

    assert_eq!(scope_changed(&changed, &target), vec!["src/main.rs"]);
}

#[test]
fn a_target_without_scope_covers_everything() {
    let target = Target::workspace_target();

    assert!(covers("anything/at/all.rs", &target));
}

fn auto_targets() -> BTreeMap<String, Target> {
    let mut targets = BTreeMap::new();
    targets.insert(
        "workspace".to_string(),
        Target {
            name: "workspace".to_string(),
            path: String::new(),
            scope: vec!["cli/".to_string(), "api/".to_string(), "lib/".to_string()],
            manifest: Some("Cargo.toml".to_string()),
            workspace_member: true,
        },
    );
    targets.insert(
        "frontend".to_string(),
        Target::crate_target("frontend", false),
    );
    targets
}

#[test]
fn auto_target_picks_the_scope_that_covers_every_change() {
    let targets = auto_targets();

    assert_eq!(
        pick_auto_target(&targets, &["frontend/src/main.rs".to_string()])
            .expect("one owner")
            .expect("a target")
            .name,
        "frontend"
    );
    assert_eq!(
        pick_auto_target(&targets, &["lib/src/foo.rs".to_string()])
            .expect("one owner")
            .expect("a target")
            .name,
        "workspace"
    );
}

#[test]
fn auto_target_ignores_files_no_target_owns() {
    let targets = auto_targets();
    let changed = [
        "lib/src/foo.rs".to_string(),
        "README.md".to_string(),
        "docs/future.md".to_string(),
    ];

    assert_eq!(
        pick_auto_target(&targets, &changed)
            .expect("one owner")
            .expect("a target")
            .name,
        "workspace"
    );
}

#[test]
fn auto_target_returns_none_when_no_target_owns_anything() {
    let targets = auto_targets();

    assert!(pick_auto_target(&targets, &["src/main.rs".to_string()])
        .expect("no owner")
        .is_none());
}

#[test]
fn auto_target_refuses_to_measure_half_a_diff() {
    let targets = auto_targets();
    let changed = [
        "frontend/src/main.rs".to_string(),
        "lib/src/foo.rs".to_string(),
    ];

    let message = pick_auto_target(&targets, &changed)
        .expect_err("ambiguous")
        .render();

    assert!(message.contains("frontend"));
    assert!(message.contains("lib"));
    assert!(message.contains("--all"));
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

#[test]
fn strip_leaves_foreign_paths_alone() {
    let target = Target::crate_target("frontend", false);

    assert_eq!(target.strip("lib/src/foo.rs"), "lib/src/foo.rs");
    assert_eq!(target.strip("frontend/src/main.rs"), "src/main.rs");
}

#[test]
fn labels_show_the_directory() {
    assert_eq!(Target::workspace_target().label(), "workspace (./)");
    assert_eq!(
        Target::crate_target("frontend", false).label(),
        "frontend (frontend/)"
    );
}
