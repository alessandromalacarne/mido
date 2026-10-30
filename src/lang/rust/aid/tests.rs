use super::*;
use crate::test_support::{FakeRunner, MiniRepo};

fn target() -> Target {
    Target::package_target("frontend", false, MANIFEST)
}

#[test]
fn workspace_members_never_need_the_aid() {
    let probe = Outcome::new(101, "", "boom");

    assert!(!workspace_aid_needed(
        &probe,
        Path::new("/repo"),
        &Target::workspace_target(MANIFEST)
    ));
}

#[test]
fn the_aid_never_writes_outside_the_directory_it_was_pointed_at() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::default();
    let missing = Target {
        path: "gone".to_string(),
        ..Target::package_target("frontend", false, MANIFEST)
    };
    let mut out = Vec::new();

    apply_workspace_aid(&runner, &repo.root, &missing, &mut out);

    assert!(!repo.root.join("gone").exists());
    let untouched =
        std::fs::read_to_string(repo.root.join("frontend/Cargo.toml")).expect("manifest");
    assert!(!untouched.starts_with("[workspace]"));
    assert!(String::from_utf8(out).expect("utf8").is_empty());
    assert!(!runner.called_with("update-index"));
}

#[test]
fn the_aid_is_not_written_twice() {
    let repo = MiniRepo::build(None);
    let manifest = repo.root.join("frontend/Cargo.toml");
    let runner = FakeRunner::default();

    apply_workspace_aid(&runner, &repo.root, &target(), &mut Vec::new());
    let once = std::fs::read_to_string(&manifest).expect("manifest");
    apply_workspace_aid(&runner, &repo.root, &target(), &mut Vec::new());
    let twice = std::fs::read_to_string(&manifest).expect("manifest");

    assert_eq!(once.matches("[workspace]").count(), 1);
    assert_eq!(twice, once);
}

#[test]
fn a_workspace_member_needs_no_probe() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::default();

    validate_target_setup(
        &runner,
        &repo.root,
        &Target::package_target("lib", true, MANIFEST),
        false,
        &mut Vec::new(),
    )
    .expect("members are fine");

    assert!(!runner.called_with("locate-project"));
}

#[test]
fn a_workspace_member_with_a_path_never_asks_for_the_aid() {
    let repo = MiniRepo::build(None);
    let member = Target::package_target("lib", true, MANIFEST);
    // cargo answers with the outer workspace root, which for a member is fine.
    let probe = Outcome::new(0, "/repo/Cargo.toml\n", "");

    assert!(!workspace_aid_needed(&probe, &repo.root, &member));
}
