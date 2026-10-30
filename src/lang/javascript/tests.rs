use super::*;
use crate::config::validate::validate_config;
use crate::lang::Lang;
use crate::targets::Target;
use crate::test_support::{FakeRunner, MiniJsRepo};
use std::path::Path;

#[test]
fn embedded_defaults_pass_config_validation() {
    validate_config(default_config(), "", Path::new("defaults.toml"))
        .expect("the embedded defaults are a valid contract");
}

fn string_array(value: &toml::Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

#[test]
fn the_defaults_carry_the_javascript_baseline() {
    let defaults = default_config();

    assert_eq!(defaults["version"], toml::Value::Integer(1));
    assert_eq!(
        string_array(&defaults["syntax"]["format"]),
        vec!["npx", "--no-install", "prettier", "--check", "."]
    );
    assert_eq!(
        string_array(&defaults["syntax"]["lint"]),
        vec!["npx", "--no-install", "eslint", "."]
    );
    assert!(
        defaults["syntax"].get("typecheck").is_none(),
        "plain javascript has no type checker to run"
    );
    assert!(
        defaults["tests"].get("command").is_none(),
        "the suite derives the command from the package manager"
    );
    assert!(
        defaults["coverage"].get("command").is_none(),
        "the coverage command is derived from the repo's devDependencies"
    );
    assert_eq!(
        defaults["size"]["file_loc"]["fail"],
        toml::Value::Integer(500)
    );
    assert_eq!(
        defaults["mutation"]["kill_rate_min"],
        toml::Value::Integer(70)
    );
}

fn argv(items: &[&str]) -> Option<Vec<String>> {
    Some(items.iter().map(|item| (*item).to_string()).collect())
}

#[test]
fn fallback_commands_are_the_bare_npx_ones() {
    let js = Lang::JavaScript;

    assert_eq!(
        js.fallback_argv("syntax", "format"),
        argv(&["npx", "--no-install", "prettier", "--check", "."])
    );
    assert_eq!(
        js.fallback_argv("syntax", "lint"),
        argv(&["npx", "--no-install", "eslint", "."])
    );
    assert_eq!(js.fallback_argv("tests", "command"), argv(&["npm", "test"]));
    assert_eq!(
        js.fallback_argv("mutation", "command"),
        argv(&["npx", "--no-install", "stryker", "run"])
    );
}

#[test]
fn javascript_source_files_are_the_js_family() {
    for path in ["src/app.js", "app.mjs", "app.cjs", "src/widget.jsx"] {
        assert!(is_source(path), "{path} is javascript");
    }
    for path in ["src/app.ts", "src/widget.tsx", "package.json", "app.rs"] {
        assert!(!is_source(path), "{path} is not javascript");
    }
}

#[test]
fn a_missing_manifest_is_a_setup_error() {
    let repo = MiniJsRepo::build(None);
    std::fs::remove_file(repo.root.join("standalone/package.json")).expect("manifest removed");
    let target = Target::package_target("standalone", false, MANIFEST);

    let error = Lang::JavaScript
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

#[test]
fn a_present_manifest_validates_and_needs_no_aid() {
    let repo = MiniJsRepo::build(None);
    let target = Target::package_target("standalone", false, MANIFEST);

    Lang::JavaScript
        .validate_target_setup(
            &FakeRunner::default(),
            &repo.root,
            &target,
            true,
            &mut Vec::new(),
        )
        .expect("a present manifest is enough");
}

#[test]
fn no_targets_error_names_what_the_ladder_measures() {
    let repo = MiniJsRepo::build(None);

    let message = Lang::JavaScript.no_targets_error(&repo.root).render();

    assert!(message.contains("no npm package found"), "{message}");
    assert!(message.contains("package.json"), "{message}");
}
