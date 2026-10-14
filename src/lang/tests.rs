use super::*;
use crate::test_support::MiniRepo;

#[test]
fn a_root_cargo_manifest_selects_the_rust_module() {
    let repo = MiniRepo::build(None);

    assert_eq!(Lang::infer(&repo.root), Ok(Lang::Rust));
}

#[test]
fn a_root_package_manifest_selects_the_javascript_module() {
    let repo = crate::test_support::MiniJsRepo::build(None);

    assert_eq!(Lang::infer(&repo.root), Ok(Lang::JavaScript));
}

#[test]
fn cargo_wins_when_a_repo_carries_both_root_manifests() {
    let repo = MiniRepo::build(None);
    std::fs::write(
        repo.root.join("package.json"),
        "{ \"name\": \"tooling\" }\n",
    )
    .expect("manifest");

    assert_eq!(Lang::infer(&repo.root), Ok(Lang::Rust));
}

#[test]
fn every_module_is_known_by_name() {
    let names: Vec<&str> = LANGS.iter().map(|lang| lang.name()).collect();

    assert_eq!(names, vec!["rust", "javascript"]);
}

#[test]
fn the_shared_metric_tools_ship_with_every_module() {
    for lang in LANGS {
        assert_eq!(lang.size_tool(), "tokei");
        assert_eq!(lang.metrics_tool(), "rust-code-analysis");
    }
}

#[test]
fn no_recognized_manifest_is_a_setup_error_naming_the_languages() {
    let directory = tempfile::tempdir().expect("temp dir");

    let message = Lang::infer(directory.path())
        .expect_err("no module recognizes an empty directory")
        .render();

    assert!(message.contains("no language module recognizes this repo"));
    assert!(message.contains("looked for Cargo.toml, package.json"));
    assert!(message.contains("known languages: rust, javascript"));
    assert!(message.contains("--lang"));
}

#[test]
fn a_target_setup_check_is_delegated_to_the_module() {
    use crate::targets::Target;
    use crate::test_support::FakeRunner;

    let repo = MiniRepo::build(None);
    std::fs::remove_file(repo.root.join("frontend/Cargo.toml")).expect("manifest removed");
    let target = Target::package_target("frontend", false, "Cargo.toml");

    let error = Lang::Rust
        .validate_target_setup(
            &FakeRunner::default(),
            &repo.root,
            &target,
            false,
            &mut Vec::new(),
        )
        .expect_err("the missing manifest must surface through the module");

    assert!(error.render().contains("has no manifest"));
}
