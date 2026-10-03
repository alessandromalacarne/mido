use super::*;
use crate::config::validate::validate_config;
use std::path::Path;

#[test]
fn embedded_defaults_match_the_repo_config_minus_script() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.mido.toml"))
        .expect(".mido.toml is there");
    let mut repo: toml::Table = text.parse().expect(".mido.toml parses");

    repo.remove("script");

    assert_eq!(
        *default_config(),
        repo,
        "the embedded defaults and the repo config must not drift"
    );
}

#[test]
fn the_defaults_carry_the_rust_baseline() {
    let defaults = default_config();

    assert_eq!(defaults["version"], toml::Value::Integer(1));
    assert_eq!(
        defaults["tests"]["command"],
        toml::Value::Array(
            ["cargo", "test", "--all-features"]
                .iter()
                .map(|arg| toml::Value::String((*arg).to_string()))
                .collect()
        )
    );
    assert_eq!(
        defaults["size"]["file_loc"]["fail"],
        toml::Value::Integer(500)
    );
}

#[test]
fn embedded_defaults_pass_config_validation() {
    validate_config(default_config(), "", Path::new("defaults.toml"))
        .expect("the embedded defaults are a valid contract");
}

fn argv(items: &[&str]) -> Option<Vec<String>> {
    Some(items.iter().map(|item| (*item).to_string()).collect())
}

#[test]
fn fallback_commands_are_the_bare_cargo_ones() {
    let rust = crate::lang::Lang::Rust;

    assert_eq!(
        rust.fallback_argv("syntax", "format"),
        argv(&["cargo", "fmt", "--check"])
    );
    assert_eq!(
        rust.fallback_argv("syntax", "lint"),
        argv(&["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    );
    assert_eq!(
        rust.fallback_argv("syntax", "typecheck"),
        argv(&["cargo", "check"])
    );
    assert_eq!(
        rust.fallback_argv("tests", "command"),
        argv(&["cargo", "test"])
    );
    assert_eq!(
        rust.fallback_argv("coverage", "command"),
        argv(&["cargo", "llvm-cov", "--lcov", "--output-path", "{lcov}"])
    );
    assert_eq!(
        rust.fallback_argv("mutation", "command"),
        argv(&["cargo", "mutants"])
    );
}

#[test]
fn keys_that_have_no_fallback_stay_unset() {
    let rust = crate::lang::Lang::Rust;

    assert_eq!(rust.fallback_argv("syntax", "command"), None);
    assert_eq!(rust.fallback_argv("mutation", "scope"), None);
    assert_eq!(rust.fallback_argv("size", "command"), None);
    assert_eq!(rust.fallback_argv("analysis", "command"), None);
}
