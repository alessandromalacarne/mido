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
