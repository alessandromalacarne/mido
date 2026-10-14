use super::*;

fn work_dir() -> &'static Path {
    Path::new("/repo/packages/alpha")
}

#[test]
fn prettier_lists_the_files_it_would_rewrite() {
    let outcome = syntax_outcome(
        "format",
        "npx --no-install prettier --check .",
        "[warn] src/app.js\n[warn] src/util.js\n[warn] Code style issues found in 2 files. Forgot to run Prettier?\n",
        1,
        work_dir(),
    );

    assert_eq!(outcome.problems.len(), 2);
    assert!(outcome.problems[0].contains("src/app.js"));
    assert!(outcome.problems[0].contains("formatting differs"));
    assert_eq!(
        outcome.details[0],
        "format: 2 file(s) with diffs -> src/app.js, src/util.js"
    );
}

#[test]
fn a_clean_format_pass_has_nothing_to_report() {
    let outcome = syntax_outcome(
        "format",
        "npx --no-install prettier --check .",
        "Checking formatting...\nAll matched files use Prettier code style!\n",
        0,
        work_dir(),
    );

    assert!(outcome.problems.is_empty());
    assert_eq!(outcome.details[0], "format: 0 file(s) with diffs");
    assert_eq!(
        syntax_outcome("format", "cmd", "", 0, work_dir()).details[0],
        "format: 0 file(s) with diffs"
    );
}

#[test]
fn a_prettier_error_is_the_problem() {
    let outcome = syntax_outcome(
        "format",
        "npx --no-install prettier --check .",
        "[error] Invalid configuration file `prettier.config.js`\n",
        2,
        work_dir(),
    );

    assert_eq!(
        outcome.problems,
        vec!["prettier: Invalid configuration file `prettier.config.js`"]
    );
}

#[test]
fn a_formatter_failure_with_no_parseable_output_is_not_a_pass() {
    let outcome = syntax_outcome(
        "format",
        "npx --no-install prettier --check .",
        "some unexpected crash text\n",
        2,
        work_dir(),
    );

    assert_eq!(outcome.problems.len(), 1);
    assert!(outcome.problems[0].contains("exited 2"));
    assert!(outcome
        .details
        .iter()
        .any(|line| line.starts_with("format: some unexpected crash")));
}

#[test]
fn eslint_entries_are_read_under_their_file_header() {
    let output = "\
/repo/packages/alpha/src/app.js
  1:7  error    'unused' is assigned a value but never used  no-unused-vars
  2:1  warning  Unexpected console statement                no-console

✖ 2 problems (1 error, 1 warning)
";
    let outcome = syntax_outcome("lint", "npx --no-install eslint .", output, 1, work_dir());

    assert_eq!(
        outcome.problems,
        vec![
            "src/app.js:1:7 error 'unused' is assigned a value but never used (no-unused-vars)",
            "src/app.js:2:1 warning Unexpected console statement (no-console)",
        ]
    );
    assert_eq!(outcome.details[0], "lint: 2 diagnostic(s) -> src/app.js");
}

#[test]
fn an_eslint_entry_without_a_rule_keeps_its_message() {
    let output = "\
/repo/packages/alpha/src/broken.js
  1:1  error  Parsing error: Unexpected token

✖ 1 problem (1 error, 0 warnings)
";
    let outcome = syntax_outcome("lint", "npx --no-install eslint .", output, 1, work_dir());

    assert_eq!(
        outcome.problems,
        vec!["src/broken.js:1:1 error Parsing error: Unexpected token"]
    );
}

#[test]
fn a_path_outside_the_target_stays_as_printed() {
    let output = "\
/elsewhere/app.js
  1:1  error  boom  rule

✖ 1 problem (1 error, 0 warnings)
";
    let outcome = syntax_outcome("lint", "npx --no-install eslint .", output, 1, work_dir());

    assert_eq!(
        outcome.problems,
        vec!["/elsewhere/app.js:1:1 error boom (rule)"]
    );
}

#[test]
fn a_clean_lint_pass_reports_nothing() {
    let outcome = syntax_outcome("lint", "npx --no-install eslint .", "", 0, work_dir());

    assert!(outcome.problems.is_empty());
    assert_eq!(outcome.details[0], "lint: 0 diagnostic(s)");
}

#[test]
fn tsc_diagnostics_are_read_in_both_shapes() {
    let plain =
        "src/app.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.\n";
    let pretty =
        "src/app.ts:12:5 - error TS2322: Type 'string' is not assignable to type 'number'.\n";

    let from_plain = syntax_outcome(
        "typecheck",
        "npx --no-install tsc --noEmit",
        plain,
        1,
        work_dir(),
    );
    let from_pretty = syntax_outcome(
        "typecheck",
        "npx --no-install tsc --noEmit",
        pretty,
        1,
        work_dir(),
    );

    assert_eq!(
        from_plain.problems,
        vec!["src/app.ts:12:5 error TS2322: Type 'string' is not assignable to type 'number'."]
    );
    assert_eq!(from_plain.problems, from_pretty.problems);
}

#[test]
fn a_failing_lint_with_no_diagnostics_is_not_a_pass() {
    let outcome = syntax_outcome(
        "lint",
        "npx --no-install eslint .",
        "Oops! Something went wrong!\n",
        2,
        work_dir(),
    );

    assert_eq!(outcome.problems.len(), 1);
    assert!(outcome.problems[0].contains("exited 2"));
    assert!(outcome
        .details
        .iter()
        .any(|line| line.starts_with("lint: Oops!")));
}

#[test]
fn the_same_finding_from_two_tools_is_kept_once() {
    let problems = dedupe_problems(&[
        "src/app.js:1:7 error 'unused' is assigned a value (no-unused-vars)".to_string(),
        "src/app.js:1:7 warning 'unused' is assigned a value (other-rule)".to_string(),
    ]);

    assert_eq!(problems.len(), 1);
    assert_eq!(
        problems[0],
        "src/app.js:1:7 warning 'unused' is assigned a value (other-rule)"
    );
}

#[test]
fn different_findings_are_both_kept() {
    let problems = dedupe_problems(&[
        "src/app.js:1:7 error 'unused' is assigned a value (no-unused-vars)".to_string(),
        "src/app.js:4:1 warning Unexpected console statement (no-console)".to_string(),
    ]);

    assert_eq!(problems.len(), 2);
}

#[test]
fn a_key_folds_the_severity_and_the_rule() {
    assert_eq!(
        diagnostic_key("src/app.js:1:7 error 'unused' is assigned a value (no-unused-vars)"),
        "src/app.js:1:7 'unused' is assigned a value"
    );
    assert_eq!(
        diagnostic_key("src/app.js: formatting differs (the formatter would rewrite it)"),
        "src/app.js: formatting differs (the formatter would rewrite it)"
    );
}
