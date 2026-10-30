use super::*;
use crate::lang::Lang;
use crate::test_support::MiniJsRepo;

fn targets(repo: &MiniJsRepo) -> BTreeMap<String, Target> {
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");
    detect_targets(&repo.root, &config)
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
