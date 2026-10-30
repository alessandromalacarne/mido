use super::*;

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

#[test]
fn a_test_command_carries_no_stryker_arguments() {
    assert_eq!(test_args(&argv(&["npm", "test"])), None);
}

#[test]
fn a_command_that_names_its_own_config_is_refused() {
    let error = pass_args(
        &argv(&[
            "npx",
            "--no-install",
            "stryker",
            "run",
            "stryker.config.json",
        ]),
        &[],
        false,
        Path::new("/run"),
        None,
    )
    .expect_err("a conflict");

    assert!(error.render().contains("names its own stryker config"));
}
