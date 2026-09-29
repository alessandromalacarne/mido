use super::*;
use crate::test_support::FakeRunner;

#[test]
fn dev_runs_the_argv_directly_when_cargo_is_on_path() {
    let runner = FakeRunner::default().tool("cargo");

    let outcome = dev(
        &runner,
        Path::new("/repo/frontend"),
        &["cargo".to_string(), "test".to_string()],
        None,
    );

    assert!(outcome.ok());
    let calls = runner.calls.borrow();
    let call = calls.first().expect("one call");
    assert_eq!(call.args, vec!["cargo", "test"]);
    assert_eq!(call.cwd, PathBuf::from("/repo/frontend"));
    assert_eq!(call.stdin, None);
    assert_eq!(call.stdout_file, None);
    assert_eq!(call.stderr_file, None);
}

#[test]
fn dev_falls_back_to_the_nix_shell_without_cargo() {
    let runner = FakeRunner::with(&[("nix develop", 0, "")]).tool("tokei");

    dev(&runner, Path::new("/repo"), &["tokei".to_string()], None);

    let calls = runner.calls.borrow();
    let call = calls.first().expect("one call");
    assert_eq!(call.args, vec!["nix", "develop", "-c", "tokei"]);
    assert_eq!(call.cwd, PathBuf::from("/repo"));
    assert!(call.stdout_file.is_some());
    assert!(call.stderr_file.is_some());
    assert_eq!(call.stdin, None);
}

#[test]
fn dev_keeps_the_runner_streams_when_the_capture_files_are_gone() {
    let runner = FakeRunner::with(&[("nix develop", 7, "tool output")]).tool("tokei");

    let outcome = dev(&runner, Path::new("/repo"), &["tokei".to_string()], None);

    assert_eq!(outcome.code, 7);
    assert_eq!(outcome.stdout, "tool output");
}

#[test]
fn exec_redirects_the_streams_when_files_are_asked_for() {
    let directory = tempfile::tempdir().expect("temp dir");
    let out_path = directory.path().join("out");
    let err_path = directory.path().join("err");

    let outcome = SystemRunner.exec(
        &Command::new("/", ["sh", "-c", "printf out; printf err >&2"])
            .stdout_file(out_path.clone())
            .stderr_file(err_path.clone()),
    );

    assert!(outcome.ok());
    assert_eq!(outcome.stdout, "");
    assert_eq!(outcome.stderr, "");
    assert_eq!(std::fs::read_to_string(&out_path).expect("stdout"), "out");
    assert_eq!(std::fs::read_to_string(&err_path).expect("stderr"), "err");
}

#[test]
fn git_returns_stdout_only_on_success() {
    let runner = FakeRunner::with(&[("rev-parse", 128, "fatal: not a repo")]);
    assert_eq!(git(&runner, Path::new("/repo"), &["rev-parse", "HEAD"]), "");

    let runner = FakeRunner::with(&[("rev-parse", 0, "abc123\n")]);
    assert_eq!(
        git(&runner, Path::new("/repo"), &["rev-parse", "HEAD"]),
        "abc123\n"
    );
}

#[test]
fn changed_files_merges_and_dedupes_every_source() {
    let runner = FakeRunner::with(&[
        ("merge-base", 0, "base\n"),
        ("diff --name-only base", 0, "a.rs\nb.rs\n"),
        ("--cached", 0, "b.rs\n"),
        ("--others", 0, "c.rs\n"),
    ]);

    assert_eq!(
        changed_files(&runner, Path::new("/repo"), "origin/mvp"),
        vec!["a.rs", "b.rs", "c.rs"]
    );
}

#[test]
fn pick_base_prefers_the_first_known_remote() {
    let runner = FakeRunner::with(&[("origin/develop", 0, "deadbeef\n"), ("origin/mvp", 128, "")]);

    assert_eq!(pick_base(&runner, Path::new("/repo")), "origin/develop");

    let runner = FakeRunner::with(&[
        ("origin/mvp", 128, ""),
        ("origin/develop", 128, ""),
        ("origin/master", 128, ""),
    ]);
    assert_eq!(pick_base(&runner, Path::new("/repo")), "HEAD");
}

#[test]
fn dirty_hash_hashes_the_short_status() {
    let runner = FakeRunner::with(&[("hash-object", 0, "abc123\n")]);

    assert_eq!(dirty_hash(&runner, Path::new("/repo")), "abc123");
    let calls = runner.calls.borrow();
    assert_eq!(calls[1].stdin.as_deref(), Some(""));
}

#[test]
fn last_lines_keeps_the_tail_of_an_output() {
    assert_eq!(last_lines("one\ntwo\nthree\n\n", 2), vec!["two", "three"]);
    assert_eq!(last_lines("only", 10), vec!["only"]);
    assert!(last_lines("", 3).is_empty());
}

#[test]
fn missing_program_is_a_not_found_exit() {
    let outcome = SystemRunner.exec(&Command::new("/", Vec::<String>::new()));

    assert_eq!(outcome.code, NOT_FOUND_EXIT);
}

#[test]
fn a_program_that_does_not_exist_is_a_not_found_exit() {
    let outcome = SystemRunner.exec(&Command::new("/", ["definitely-not-a-real-tool-xyzzy"]));

    assert_eq!(outcome.code, NOT_FOUND_EXIT);
    assert!(!outcome.stderr.is_empty());
}

#[test]
fn exec_reports_the_exit_code_and_both_streams() {
    let outcome = SystemRunner.exec(&Command::new(
        "/",
        ["sh", "-c", "printf out; printf err >&2; exit 3"],
    ));

    assert_eq!(outcome.code, 3);
    assert_eq!(outcome.stdout, "out");
    assert_eq!(outcome.stderr, "err");
    assert!(!outcome.ok());
    assert_eq!(outcome.combined(), "outerr");
}

#[test]
fn exec_hands_stdin_to_the_program() {
    let outcome = SystemRunner.exec(&Command::new("/", ["sh", "-c", "cat"]).stdin("hello"));

    assert_eq!(outcome.stdout, "hello");
    assert!(outcome.ok());
}

#[test]
fn a_program_killed_by_a_signal_reports_128_plus_the_signal() {
    let outcome = SystemRunner.exec(&Command::new("/", ["sh", "-c", "kill -TERM $$"]));

    assert_eq!(outcome.code, 128 + 15);
}

#[test]
fn a_program_that_overruns_its_timeout_is_killed() {
    let outcome = SystemRunner.exec(&Command::new("/", ["sh", "-c", "sleep 5"]).timeout(Some(1)));

    assert_eq!(outcome.code, TIMEOUT_EXIT);
    assert_eq!(outcome.stderr, "timed out after 1s");
}

#[test]
fn a_program_inside_its_timeout_finishes_normally() {
    let outcome =
        SystemRunner.exec(&Command::new("/", ["sh", "-c", "sleep 0.2; exit 0"]).timeout(Some(10)));

    assert_eq!(outcome.code, 0);
}

#[test]
fn a_scratch_directory_is_created_under_the_temp_dir() {
    let directory = scratch_directory("guardrails-test-");

    assert!(directory.is_dir());
    assert!(directory.starts_with(std::env::temp_dir()));

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn only_executable_files_count_as_tools() {
    let directory = tempfile::tempdir().expect("temp dir");
    let file = directory.path().join("maybe-a-tool");
    std::fs::write(&file, "#!/bin/sh\n").expect("file");

    assert!(!is_executable(&file));
    assert!(!is_executable(&directory.path().join("ghost")));
    assert!(!is_executable(directory.path()));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&file).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&file, permissions).expect("permissions");
        assert!(is_executable(&file));
    }
}

#[test]
fn the_path_is_searched_for_tools() {
    assert!(SystemRunner.has("sh"));
    assert!(!SystemRunner.has("definitely-not-a-real-tool-xyzzy"));
}
