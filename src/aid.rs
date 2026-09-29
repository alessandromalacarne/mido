//! The workspace aid — a nested worktree cannot resolve the outer workspace.

use crate::error::GuardrailsError;
use crate::process::{self, Outcome, Runner};
use crate::targets::Target;
use std::io::Write;
use std::path::Path;

/// Ask cargo which manifest it takes as the workspace root from the target dir.
pub fn probe_workspace(runner: &dyn Runner, repo: &Path, target: &Target) -> Outcome {
    process::dev(
        runner,
        &target.dir(repo),
        &[
            "cargo".to_string(),
            "locate-project".to_string(),
            "--workspace".to_string(),
            "--message-format".to_string(),
            "plain".to_string(),
        ],
        None,
    )
}

/// Whether cargo cannot resolve the crate's own workspace root from `target.dir()`.
///
/// Excluded crates are fine in the main checkout but not in a nested worktree:
/// the root manifest lists `members`, so cargo refuses the package (exit 101,
/// "believes it's in a workspace when it's not") or silently answers with the
/// outer workspace root. Both mean the local `[workspace]` aid is required.
pub fn workspace_aid_needed(probe: &Outcome, repo: &Path, target: &Target) -> bool {
    if target.workspace_member || target.path.is_empty() {
        return false;
    }
    if !probe.ok() {
        return true;
    }

    let located: Vec<&str> = probe
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let Some(last) = located.last() else {
        return true;
    };

    let answer =
        std::fs::canonicalize(last.trim()).unwrap_or_else(|_| Path::new(last.trim()).to_path_buf());
    let expected = repo.join(target.manifest.clone().unwrap_or_default());
    let expected = std::fs::canonicalize(&expected).unwrap_or(expected);
    answer != expected
}

/// The marker the aid writes, and checks for, so a second run is a no-op.
const AID_MARKER: &str = "# Local worktree aid (not for commit)";
const AID_HEADER: &str =
    "# Local worktree aid (not for commit): declares this package as its own\n\
                           # workspace root, so cargo does not walk up into the main checkout.\n\
                           [workspace]\n\n";

pub fn apply_workspace_aid(runner: &dyn Runner, repo: &Path, target: &Target, out: &mut dyn Write) {
    let manifest = target.dir(repo).join("Cargo.toml");
    // The aid edits exactly one file: the target's own manifest, at an absolute
    // path, in a directory that already exists. Anything else means the caller is
    // confused, and a manifest is not the way to find out.
    let inside_a_directory = manifest
        .parent()
        .is_some_and(|directory| directory.is_absolute() && directory.is_dir());
    if !inside_a_directory {
        return;
    }

    if let Ok(text) = std::fs::read_to_string(&manifest) {
        if !text.starts_with(AID_MARKER) && !text.starts_with("[workspace]") {
            let updated = format!("{AID_HEADER}{text}");
            // Additive only: the aid never loses a line of the manifest it edits.
            if updated.ends_with(&text) {
                let _ = std::fs::write(&manifest, updated);
            }
        }
    }

    let manifest = target
        .manifest
        .clone()
        .unwrap_or_else(|| "Cargo.toml".to_string());
    let _ = process::git(
        runner,
        repo,
        &["update-index", "--skip-worktree", &manifest],
    );
    let _ = writeln!(
        out,
        "workspace aid applied: {manifest} declares its own [workspace] (skip-worktree)"
    );
}

pub fn validate_target_setup(
    runner: &dyn Runner,
    repo: &Path,
    target: &Target,
    apply_aid: bool,
    out: &mut dyn Write,
) -> Result<(), GuardrailsError> {
    ensure_manifest(repo, target)?;

    if target.workspace_member || target.path.is_empty() {
        return Ok(());
    }

    let probe = probe_workspace(runner, repo, target);
    if probe.code == process::NOT_FOUND_EXIT {
        return Err(cargo_missing_error(&probe));
    }
    if !workspace_aid_needed(&probe, repo, target) {
        return Ok(());
    }
    if apply_aid {
        apply_workspace_aid(runner, repo, target, out);
        return Ok(());
    }
    Err(aid_needed_error(target, &probe))
}

fn ensure_manifest(repo: &Path, target: &Target) -> Result<(), GuardrailsError> {
    let Some(manifest) = &target.manifest else {
        return Ok(());
    };
    let path = repo.join(manifest);
    if path.exists() {
        return Ok(());
    }

    Err(
        GuardrailsError::setup(format!("target `{}` has no manifest", target.name))
            .detail(format!("looked for {}", path.display()))
            .hint("pass --list-targets to see what can be measured"),
    )
}

fn cargo_missing_error(probe: &Outcome) -> GuardrailsError {
    let details = if probe.stderr.trim().is_empty() {
        vec!["`cargo locate-project` could not run".to_string()]
    } else {
        process::last_lines(&probe.stderr, 3)
    };

    GuardrailsError::setup("cargo is not available")
        .details(details)
        .hint("run the ladder inside `nix develop`, or install the Rust toolchain")
}

fn aid_needed_error(target: &Target, probe: &Outcome) -> GuardrailsError {
    let mut details = vec![
        "this happens in a worktree nested under .agents/, where cargo walks up into the main checkout instead"
            .to_string(),
    ];
    details.extend(probe.stderr.trim().lines().take(4).map(str::to_string));

    GuardrailsError::setup(format!(
        "cargo cannot resolve `{}`'s workspace root",
        target.path
    ))
    .details(details)
    .hint("re-run with --apply-workspace-aid to add the git-invisible [workspace] line")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Outcome;
    use crate::test_support::{FakeRunner, MiniRepo};
    use std::path::PathBuf;

    fn target() -> Target {
        Target::crate_target("frontend", false)
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
            &Target::workspace_target()
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
            ..Target::crate_target("frontend", false)
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
            &Target::crate_target("lib", true),
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
        let member = Target::crate_target("lib", true);
        // cargo answers with the outer workspace root, which for a member is fine.
        let probe = Outcome::new(0, "/repo/Cargo.toml\n", "");

        assert!(!workspace_aid_needed(&probe, &repo.root, &member));
    }

    #[test]
    fn only_a_standalone_crate_with_a_path_is_probed() {
        let repo = MiniRepo::build(None);
        let standalone = Target::crate_target("frontend", false);
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
}
