use super::*;
use crate::lang::Lang;
use crate::test_support::{argv, MiniJsRepo};
use std::path::PathBuf;

fn repo_with_dependencies(
    dependencies: Option<&str>,
    root_dependencies: Option<&str>,
) -> MiniJsRepo {
    let repo = MiniJsRepo::build(None);
    if let Some(dependencies) = dependencies {
        std::fs::write(
            repo.root.join("standalone/package.json"),
            format!("{{ \"name\": \"standalone-cli\", \"devDependencies\": {dependencies} }}\n"),
        )
        .expect("package.json");
    }
    if let Some(root_dependencies) = root_dependencies {
        std::fs::write(
            repo.root.join("package.json"),
            format!(
                "{{ \"name\": \"mini\", \"private\": true, \"workspaces\": [\"packages/*\", \"tools\"], \"devDependencies\": {root_dependencies} }}\n"
            ),
        )
        .expect("package.json");
    }
    repo
}

fn standalone(repo: &MiniJsRepo) -> (Config, Target) {
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");
    let target = Target::package_target("standalone", false, "package.json");

    (config, target)
}

#[test]
fn vitest_dependencies_get_vitests_own_coverage() {
    let repo = repo_with_dependencies(Some("{ \"vitest\": \"^3\" }"), None);
    let (config, target) = standalone(&repo);

    assert_eq!(
        command(&config, &target, &repo.root),
        Some(argv(&[
            "npx",
            "--no-install",
            "vitest",
            "run",
            "--coverage",
            "--coverage.reporter=lcov",
            "--coverage.reportsDirectory=coverage",
        ]))
    );
}

#[test]
fn jest_dependencies_get_jests_own_coverage() {
    let repo = repo_with_dependencies(Some("{ \"jest\": \"^29\" }"), None);
    let (config, target) = standalone(&repo);

    assert_eq!(
        command(&config, &target, &repo.root),
        Some(argv(&[
            "npx",
            "--no-install",
            "jest",
            "--coverage",
            "--coverageReporters=lcov",
            "--coverageDirectory=coverage",
        ]))
    );
}

#[test]
fn a_hoisted_monorepo_falls_back_to_the_root_dependencies() {
    let repo = repo_with_dependencies(None, Some("{ \"jest\": \"^29\" }"));
    let (config, target) = standalone(&repo);

    assert_eq!(
        command(&config, &target, &repo.root).map(|cmd| cmd[2].clone()),
        Some("jest".to_string())
    );
}

#[test]
fn a_targets_own_dependencies_decide_before_the_roots() {
    let repo = repo_with_dependencies(
        Some("{ \"jest\": \"^29\" }"),
        Some("{ \"vitest\": \"^3\" }"),
    );
    let (config, target) = standalone(&repo);

    assert_eq!(
        command(&config, &target, &repo.root).map(|cmd| cmd[2].clone()),
        Some("jest".to_string())
    );
}

#[test]
fn without_a_named_runner_c8_wraps_the_test_script() {
    let repo = repo_with_dependencies(None, None);
    let (config, target) = standalone(&repo);

    assert_eq!(
        command(&config, &target, &repo.root),
        Some(argv(&[
            "npx",
            "--no-install",
            "c8",
            "--reporter=lcovonly",
            "--reports-dir",
            "coverage",
            "npm",
            "test",
        ]))
    );
}

#[test]
fn the_c8_fallback_wraps_the_package_managers_test_script() {
    let repo = repo_with_dependencies(None, None);
    std::fs::write(repo.root.join("pnpm-lock.yaml"), "").expect("lockfile");
    let (config, target) = standalone(&repo);

    assert_eq!(
        command(&config, &target, &repo.root).map(|argv| argv[argv.len() - 2].clone()),
        Some("pnpm".to_string())
    );
}

#[test]
fn a_configured_command_wins_over_detection() {
    let repo = repo_with_dependencies(Some("{ \"vitest\": \"^3\" }"), None);
    std::fs::write(
        repo.root.join(".mido.toml"),
        "version = 1\n\n[coverage]\ncommand = [\"npx\", \"c8\", \"npm\", \"test\"]\n",
    )
    .expect("config");
    let config = Config::load(&repo.root, &Lang::JavaScript).expect("config loads");
    let target = Target::workspace_target("package.json");

    assert_eq!(
        command(&config, &target, &repo.root),
        Some(argv(&["npx", "c8", "npm", "test"]))
    );
}

#[test]
fn a_command_without_report_flags_defaults_to_the_coverage_directory() {
    let command = argv(&["npx", "--no-install", "vitest", "run", "--coverage"]);

    assert_eq!(
        report_path(&command),
        Some(PathBuf::from("coverage").join("lcov.info"))
    );
}

#[test]
fn the_report_directory_flag_decides_the_report_path() {
    for command in [
        argv(&[
            "npx",
            "vitest",
            "run",
            "--coverage.reportsDirectory=reports",
        ]),
        argv(&["npx", "jest", "--coverageDirectory=reports"]),
        argv(&["npx", "c8", "--reports-dir", "reports", "npm", "test"]),
        argv(&["npx", "c8", "--report-dir", "reports", "npm", "test"]),
    ] {
        assert_eq!(
            report_path(&command),
            Some(PathBuf::from("reports").join("lcov.info"))
        );
    }
}
