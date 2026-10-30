use super::*;
use crate::test_support::MiniRepo;

#[test]
fn every_module_is_known_by_name() {
    let names: Vec<&str> = LANGS.iter().map(|lang| lang.name()).collect();

    assert_eq!(names, vec!["rust", "typescript", "javascript"]);
}

#[test]
fn every_module_names_the_tools_it_measures_with() {
    assert_eq!(Lang::Rust.size_tool(), "tokei");
    assert_eq!(Lang::Rust.metrics_tool(), "rust-code-analysis");

    // The node modules measure the same way as each other, not as rust.
    for lang in [Lang::TypeScript, Lang::JavaScript] {
        assert_eq!(lang.size_tool(), "eslint");
        assert_eq!(lang.metrics_tool(), "eslint");
    }
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
