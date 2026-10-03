use super::*;
use crate::test_support::MiniRepo;

#[test]
fn a_root_cargo_manifest_selects_the_rust_module() {
    let repo = MiniRepo::build(None);

    assert_eq!(Lang::infer(&repo.root), Ok(Lang::Rust));
}

#[test]
fn no_recognized_manifest_is_a_setup_error_naming_the_languages() {
    let directory = tempfile::tempdir().expect("temp dir");

    let message = Lang::infer(directory.path())
        .expect_err("no module recognizes an empty directory")
        .render();

    assert!(message.contains("no language module recognizes this repo"));
    assert!(message.contains("known languages: rust"));
    assert!(message.contains("--lang rust"));
}

#[test]
fn a_target_setup_check_is_delegated_to_the_module() {
    use crate::targets::Target;
    use crate::test_support::FakeRunner;

    let repo = MiniRepo::build(None);
    std::fs::remove_file(repo.root.join("frontend/Cargo.toml")).expect("manifest removed");
    let target = Target::crate_target("frontend", false, "Cargo.toml");

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
