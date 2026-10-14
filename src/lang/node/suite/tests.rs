use super::*;
use crate::lang::Lang;
use crate::test_support::MiniJsRepo;

fn config_for(repo: &MiniJsRepo) -> Config {
    Config::load(&repo.root, &Lang::JavaScript).expect("config loads")
}

fn target() -> Target {
    Target::workspace_target("package.json")
}

#[test]
fn the_default_suite_is_npm_test() {
    let repo = MiniJsRepo::build(None);

    assert_eq!(
        test_commands(&config_for(&repo), &target(), &repo.root),
        vec![vec!["npm".to_string(), "test".to_string()]]
    );
}

#[test]
fn the_lockfile_names_the_package_manager() {
    let repo = MiniJsRepo::build(None);

    std::fs::write(repo.root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").expect("lockfile");
    assert_eq!(
        test_commands(&config_for(&repo), &target(), &repo.root),
        vec![vec!["pnpm".to_string(), "test".to_string()]]
    );

    std::fs::remove_file(repo.root.join("pnpm-lock.yaml")).expect("lockfile");
    std::fs::write(repo.root.join("yarn.lock"), "# yarn lockfile v1\n").expect("lockfile");
    assert_eq!(
        test_commands(&config_for(&repo), &target(), &repo.root),
        vec![vec!["yarn".to_string(), "test".to_string()]]
    );

    std::fs::remove_file(repo.root.join("yarn.lock")).expect("lockfile");
    std::fs::write(repo.root.join("bun.lockb"), "binary\n").expect("lockfile");
    assert_eq!(
        test_commands(&config_for(&repo), &target(), &repo.root),
        vec![vec!["bun".to_string(), "test".to_string()]]
    );

    std::fs::remove_file(repo.root.join("bun.lockb")).expect("lockfile");
    std::fs::write(repo.root.join("package-lock.json"), "{}\n").expect("lockfile");
    assert_eq!(
        test_commands(&config_for(&repo), &target(), &repo.root),
        vec![vec!["npm".to_string(), "test".to_string()]]
    );
}

#[test]
fn a_lockfile_next_to_the_target_wins_over_the_root() {
    let repo = MiniJsRepo::build(None);
    std::fs::write(repo.root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").expect("lockfile");
    std::fs::write(repo.root.join("packages/alpha/package-lock.json"), "{}\n").expect("lockfile");
    let alpha = Target::package_target("packages/alpha", true, "package.json");

    assert_eq!(
        test_commands(&config_for(&repo), &alpha, &repo.root),
        vec![vec!["npm".to_string(), "test".to_string()]]
    );
}

#[test]
fn a_configured_command_wins_over_the_lockfile() {
    let repo = MiniJsRepo::build(Some(
        "
        version = 1

        [tests]
        command = [\"pnpm\", \"-r\", \"test\"]
    ",
    ));
    std::fs::write(repo.root.join("package-lock.json"), "{}\n").expect("lockfile");

    assert_eq!(
        test_commands(&config_for(&repo), &target(), &repo.root),
        vec![vec![
            "pnpm".to_string(),
            "-r".to_string(),
            "test".to_string()
        ]]
    );
}

#[test]
fn node_test_spec_output_is_summed() {
    let output = "\
✔ works (0.65ms)
ℹ tests 3
ℹ suites 0
ℹ pass 2
ℹ fail 1
ℹ cancelled 0
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(summary.passed, 2);
    assert_eq!(summary.failed, 1);
}

#[test]
fn node_test_failures_are_named() {
    let output = "\
✔ passes (0.7ms)
✖ fails badly (0.7ms)
✖ suite (17.3ms)
ℹ tests 3
ℹ pass 1
ℹ fail 2

✖ failing tests:

test at sample.test.js:4:1
✖ fails badly (0.700734ms)
  AssertionError [ERR_ASSERTION]: 1 == 2

test at sample.test.js:6:11
✖ nested (with parens) fails (16.923206ms)
  AssertionError: false == true
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(
        summary.failed_names,
        vec!["fails badly", "nested (with parens) fails"]
    );
}

#[test]
fn node_test_tap_output_is_read() {
    let output = "\
TAP version 13
# Subtest: passes
ok 1 - passes
not ok 2 - fails
# tests 2
# pass 1
# fail 1
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.failed_names, vec!["fails"]);
}

#[test]
fn vitest_output_is_read() {
    let output = "\
 ❯ test/calc.test.js (2 tests | 1 failed) 12ms
   × adds

⎯⎯⎯⎯⎯⎯ Failed Tests 1 ⎯⎯⎯⎯⎯⎯

 FAIL  test/calc.test.js > adds
AssertionError: expected 1 to be 2

 Test Files  1 failed (1)
      Tests  1 failed | 1 passed (2)
   Duration  300ms
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.failed_names, vec!["test/calc.test.js > adds"]);
}

#[test]
fn vitest_all_passing_output_is_read() {
    let output = "\
 Test Files  1 passed (1)
      Tests  2 passed (2)
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(summary.passed, 2);
    assert_eq!(summary.failed, 0);
    assert!(summary.failed_names.is_empty());
}

#[test]
fn jest_output_is_read() {
    let output = "\
PASS src/calc.test.js
FAIL src/other.test.js
  ● other › breaks

Tests:       1 failed, 3 passed, 4 total
Test Suites: 1 failed, 1 passed, 2 total
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(summary.passed, 3);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.failed_names, vec!["other › breaks"]);
}

#[test]
fn a_failed_jest_suite_is_not_named_as_a_test() {
    let output = "\
  ● Test suite failed to run
    Cannot find module './missing'

Tests:       2 passed, 2 total
";
    let summary = test_summary(output).expect("a summary");

    assert_eq!(summary.passed, 2);
    assert!(summary.failed_names.is_empty());
}

#[test]
fn output_without_a_summary_is_nothing() {
    assert_eq!(test_summary("npm error Missing script: \"test\"\n"), None);
    assert_eq!(test_summary(""), None);
}
