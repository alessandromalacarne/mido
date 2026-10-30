//! How the layers compose: module defaults, the repo file, and target sections.

use super::{load, repo};

#[test]
fn a_target_section_is_merged_into_the_root_section() {
    let repo = repo(
        "
            version = 1

            [size]
            file_loc = { warn = 300, fail = 500 }
            nesting = { warn = 3, fail = 4 }

            [targets.frontend.size]
            file_loc = { warn = 100, fail = 200 }
        ",
    );
    let config = load(&repo).expect("config loads");

    let frontend = config.section("size", Some("frontend"));
    assert_eq!(frontend["file_loc"]["fail"], toml::Value::Integer(200));
    assert_eq!(frontend["nesting"]["fail"], toml::Value::Integer(4));

    let workspace = config.section("size", Some("workspace"));
    assert_eq!(workspace["file_loc"]["fail"], toml::Value::Integer(500));
}
