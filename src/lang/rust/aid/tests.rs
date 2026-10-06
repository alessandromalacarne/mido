use super::*;
use crate::test_support::{FakeRunner, MiniRepo};
use std::path::PathBuf;

fn target() -> Target {
    Target::crate_target("frontend", false, MANIFEST)
}

#[test]
fn crate_resolving_its_own_manifest_needs_no_aid() {
    let probe = Outcome::new(0, "/repo/frontend/Cargo.toml\n", "");

    assert!(!workspace_aid_needed(&probe, Path::new("/repo"), &target()));
}

#[test]
fn outer_workspace_root_asks_for_the_aid() {
    let probe = Outcome::new(0, "/repo/Cargo.toml\n", "");

    assert!(workspace_aid_needed(&probe, Path::new("/repo"), &target()));
}

#[test]
fn cargo_refusing_the_package_asks_for_the_aid() {
    let probe = Outcome::new(
        101,
        "",
        "error: current package believes it's in a workspace when it's not:",
    );

    assert!(workspace_aid_needed(&probe, Path::new("/repo"), &target()));
}

#[test]
fn silent_success_asks_for_the_aid() {
    let probe = Outcome::new(0, "\n", "");

    assert!(workspace_aid_needed(&probe, Path::new("/repo"), &target()));
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
fn a_missing_manifest_is_a_setup_error() {
    let repo = MiniRepo::build(None);
    std::fs::remove_file(repo.root.join("frontend/Cargo.toml")).expect("manifest removed");

    let message = validate_target_setup(
        &FakeRunner::default(),
        &repo.root,
        &target(),
        false,
        &mut Vec::new(),
    )
    .expect_err("no manifest")
    .render();

    assert!(message.contains("has no manifest"));
    assert!(message.contains("--list-targets"));
}

#[test]
fn a_missing_cargo_is_a_setup_error_pointing_at_the_dev_shell() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("locate-project", 127, "")]);

    let message = validate_target_setup(&runner, &repo.root, &target(), false, &mut Vec::new())
        .expect_err("no cargo")
        .render();

    assert!(message.contains("cargo is not available"));
    assert!(message.contains("nix develop"));
}

#[test]
fn an_unresolvable_workspace_asks_for_the_flag() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("locate-project", 0, "/repo/Cargo.toml\n")]);

    let message = validate_target_setup(&runner, &repo.root, &target(), false, &mut Vec::new())
        .expect_err("needs the aid")
        .render();

    assert!(message.contains("cannot resolve"));
    assert!(message.contains("--apply-workspace-aid"));
}

#[test]
fn applying_the_aid_writes_the_header_and_hides_the_manifest() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("locate-project", 0, "/repo/Cargo.toml\n")]);
    let mut out = Vec::new();

    validate_target_setup(&runner, &repo.root, &target(), true, &mut out).expect("aid applied");

    let manifest =
        std::fs::read_to_string(repo.root.join("frontend/Cargo.toml")).expect("manifest");
    assert!(manifest.starts_with(AID_MARKER));
    assert!(manifest.contains("[workspace]"));
    assert!(
        manifest.contains("[package]"),
        "the aid never loses the manifest it edits"
    );
    assert!(runner.called_with("update-index --skip-worktree frontend/Cargo.toml"));
    assert!(String::from_utf8(out)
        .expect("utf8")
        .contains("workspace aid applied"));
}

#[test]
fn the_aid_never_writes_outside_the_directory_it_was_pointed_at() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::default();
    let missing = Target {
        path: "gone".to_string(),
        ..Target::crate_target("frontend", false, MANIFEST)
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
        &Target::crate_target("lib", true, MANIFEST),
        false,
        &mut Vec::new(),
    )
    .expect("members are fine");

    assert!(!runner.called_with("locate-project"));
}

#[test]
fn a_resolvable_crate_needs_no_probe_result_beyond_itself() {
    let repo = MiniRepo::build(None);
    let own_manifest: PathBuf = repo.root.join("frontend/Cargo.toml");
    let runner = FakeRunner::with(&[(
        "locate-project",
        0,
        &format!("{}\n", own_manifest.display()),
    )]);

    validate_target_setup(&runner, &repo.root, &target(), false, &mut Vec::new())
        .expect("resolves to its own manifest");
}

#[test]
fn a_workspace_member_with_a_path_never_asks_for_the_aid() {
    let repo = MiniRepo::build(None);
    let member = Target::crate_target("lib", true, MANIFEST);
    // cargo answers with the outer workspace root, which for a member is fine.
    let probe = Outcome::new(0, "/repo/Cargo.toml\n", "");

    assert!(!workspace_aid_needed(&probe, &repo.root, &member));
}

#[test]
fn only_a_standalone_crate_with_a_path_is_probed() {
    let repo = MiniRepo::build(None);
    let standalone = Target::crate_target("frontend", false, MANIFEST);
    let own_manifest = repo.root.join("frontend/Cargo.toml");
    let answers_itself = Outcome::new(0, format!("{}\n", own_manifest.display()), "");

    assert!(!workspace_aid_needed(
        &answers_itself,
        &repo.root,
        &standalone
    ));
    assert!(workspace_aid_needed(
        &Outcome::new(0, "/repo/Cargo.toml\n", ""),
        &repo.root,
        &standalone
    ));
}
