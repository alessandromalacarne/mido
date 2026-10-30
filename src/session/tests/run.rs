//! The session as the CLI drives it: argv in, exit code and output out.

use crate::test_support::{run_cli as run, FakeRunner, MiniRepo};

#[test]
fn a_config_without_a_version_warns_on_stderr() {
    let repo = MiniRepo::build(Some(
        "
            [tests]
            command = [\"cargo\", \"test\"]
        ",
    ));
    let (code, _, err) = run(
        &["--repo", &repo.root.to_string_lossy(), "--list-targets"],
        &FakeRunner::default(),
    );

    assert_eq!(code, 0);
    assert!(err.contains("warning:"), "{err}");
    assert!(err.contains("version"), "{err}");
}

#[test]
fn a_run_with_no_visible_file_measures_nothing() {
    let repo = MiniRepo::build(None);
    let runner = FakeRunner::with(&[("hash-object", 0, "dirtyhash\n")]);

    let (code, out, _) = run(
        &["--repo", &repo.root.to_string_lossy(), "--gate", "tests"],
        &runner,
    );

    assert_eq!(code, 2, "{out}");
    assert!(out.contains("git lists no file"), "{out}");
    assert!(out.contains("nothing to measure"), "{out}");
}
