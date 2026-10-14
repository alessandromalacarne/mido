use super::*;
use crate::lang::Lang;
use crate::test_support::MiniJsRepo;

fn targets(repo: &MiniJsRepo) -> BTreeMap<String, Target> {
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");
    detect_targets(&repo.root, &config)
}

fn no_workspaces(repo: &MiniJsRepo) {
    std::fs::write(repo.root.join("package.json"), "{ \"name\": \"mini\" }\n").expect("manifest");
}

#[test]
fn the_root_package_is_the_workspace_target() {
    let repo = MiniJsRepo::build(None);

    let detected = targets(&repo);
    let workspace = detected.get("workspace").expect("workspace target");

    assert_eq!(workspace.path, "");
    assert_eq!(workspace.package, None);
    assert_eq!(workspace.manifest.as_deref(), Some("package.json"));
    assert!(workspace.workspace_member);
}

#[test]
fn pattern_members_become_targets_of_their_own() {
    let repo = MiniJsRepo::build(None);

    let detected = targets(&repo);
    let alpha = detected.get("packages/alpha").expect("a pattern member");

    assert_eq!(alpha.path, "packages/alpha");
    assert_eq!(alpha.package.as_deref(), Some("@mini/alpha"));
    assert_eq!(
        alpha.manifest.as_deref(),
        Some("packages/alpha/package.json")
    );
    assert!(alpha.workspace_member);
    assert!(detected.contains_key("packages/beta"));
    assert!(detected.contains_key("tools"), "a literal pattern member");
}

#[test]
fn a_package_outside_the_patterns_stands_alone() {
    let repo = MiniJsRepo::build(None);

    let detected = targets(&repo);
    let standalone = detected.get("standalone").expect("the standalone package");

    assert_eq!(standalone.package.as_deref(), Some("standalone-cli"));
    assert!(!standalone.workspace_member);
}

#[test]
fn node_modules_is_noise_never_a_target() {
    let repo = MiniJsRepo::build(None);

    let detected = targets(&repo);

    assert!(
        !detected.keys().any(|name| name.contains("node_modules")),
        "node_modules must never surface: {:?}",
        detected.keys().collect::<Vec<_>>()
    );
}

#[test]
fn package_resolution_takes_the_declared_name() {
    let repo = MiniJsRepo::build(None);
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");

    let target = resolve_package(&repo.root, &config, "@mini/alpha").expect("resolves");

    assert_eq!(target.name, "packages/alpha");
    assert_eq!(target.path, "packages/alpha");
}

#[test]
fn package_resolution_takes_the_target_name_for_declared_targets() {
    let repo = MiniJsRepo::build(Some(
        "
        version = 1

        [targets.tui]
        path = \"tools\"
    ",
    ));
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");

    let target = resolve_package(&repo.root, &config, "tui").expect("resolves");

    assert_eq!(target.path, "tools");
}

#[test]
fn an_unknown_package_is_a_setup_error_listing_the_known_names() {
    let repo = MiniJsRepo::build(None);
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");

    let message = resolve_package(&repo.root, &config, "nope")
        .expect_err("unknown package")
        .render();

    assert!(message.contains("package `nope` not found"), "{message}");
    assert!(message.contains("@mini/alpha"), "{message}");
    assert!(message.contains("workspace"), "{message}");
}

#[test]
fn the_object_form_of_workspaces_is_read() {
    let repo = MiniJsRepo::build(None);
    std::fs::write(
        repo.root.join("package.json"),
        "{ \"name\": \"mini\", \"workspaces\": { \"packages\": [\"tools\"] } }\n",
    )
    .expect("manifest");

    let detected = targets(&repo);

    assert!(detected.contains_key("tools"));
    assert!(
        !detected.contains_key("packages/alpha"),
        "only the listed pattern rolls up: {:?}",
        detected.keys().collect::<Vec<_>>()
    );
}

#[test]
fn a_negated_pattern_takes_matches_away() {
    let repo = MiniJsRepo::build(None);
    std::fs::write(
        repo.root.join("package.json"),
        "{ \"name\": \"mini\", \"private\": true, \
         \"workspaces\": [\"packages/*\", \"!packages/beta\"] }\n",
    )
    .expect("manifest");

    let detected = targets(&repo);
    let beta = detected
        .get("packages/beta")
        .expect("beta is still a package");

    assert!(
        !beta.workspace_member,
        "the negated pattern took beta out of the workspace"
    );
    assert!(detected["packages/alpha"].workspace_member);
}

#[test]
fn unlisted_nested_packages_need_a_declared_target() {
    let repo = MiniJsRepo::build(None);
    no_workspaces(&repo);

    let detected = targets(&repo);

    assert!(
        !detected.contains_key("packages/alpha"),
        "without a pattern, a nested package is not detected: {:?}",
        detected.keys().collect::<Vec<_>>()
    );
    assert!(
        detected.contains_key("standalone"),
        "a top-level package is still its own target"
    );
    assert!(!detected["standalone"].workspace_member);
}

#[test]
fn a_declared_target_extends_the_detected_set() {
    let repo = MiniJsRepo::build(Some(
        "
        version = 1

        [targets.site]
        path = \"packages/alpha\"
    ",
    ));

    assert_eq!(targets(&repo)["site"].path, "packages/alpha");
}

#[test]
fn targets_without_a_root_manifest_have_no_manifest() {
    let repo = MiniJsRepo::build(None);
    std::fs::remove_file(repo.root.join("package.json")).expect("no root manifest");

    let detected = targets(&repo);

    assert_eq!(detected["workspace"].manifest, None);
    assert_eq!(detected["workspace"].package, None);
}

#[test]
fn glob_segments_match_one_level_or_many() {
    assert!(matches_pattern("packages/*", "packages/alpha"));
    assert!(!matches_pattern("packages/*", "packages/alpha/nested"));
    assert!(matches_pattern("tools", "tools"));
    assert!(!matches_pattern("tools", "tools/nested"));
    assert!(matches_pattern("apps/**", "apps/web"));
    assert!(matches_pattern("apps/**", "apps/web/deep"));
    assert!(!matches_pattern("apps/**", "tools"));
    assert!(!matches_pattern("apps/*", "apps/web/deep"));
    assert!(!matches_pattern("apps/**/web", "apps"));
    assert!(matches_pattern("apps/**/web", "apps/x/web"));
}

#[test]
fn a_leading_dot_segment_is_no_segment() {
    assert!(matches_pattern("./packages/*", "packages/alpha"));
}

#[test]
fn hidden_directories_and_node_modules_are_never_packages() {
    let repo = MiniJsRepo::build(None);
    std::fs::write(
        repo.root.join("package.json"),
        "{ \"name\": \"mini\", \"private\": true, \"workspaces\": [\"*\"] }\n",
    )
    .expect("manifest");
    std::fs::create_dir_all(repo.root.join(".hidden")).expect("dir");
    std::fs::write(
        repo.root.join(".hidden/package.json"),
        "{ \"name\": \"hidden\" }\n",
    )
    .expect("manifest");

    let detected = targets(&repo);
    let names: Vec<&String> = detected.keys().collect();

    assert!(
        detected.contains_key("tools"),
        "a plain top-level package is a member: {names:?}"
    );
    assert!(!detected.contains_key(".hidden"), "{names:?}");
    assert!(
        !detected.keys().any(|name| name.contains("node_modules")),
        "{names:?}"
    );
}

#[test]
fn workspace_patterns_come_from_the_root_manifest() {
    let repo = MiniJsRepo::build(None);

    assert_eq!(
        workspace_patterns(&repo.root),
        vec!["packages/*".to_string(), "tools".to_string()]
    );

    no_workspaces(&repo);
    assert!(workspace_patterns(&repo.root).is_empty());
}
