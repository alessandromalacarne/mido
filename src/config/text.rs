use regex::Regex;
use std::sync::OnceLock;

fn assignment_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^\s*(?:\[([^\]]+)\]|([A-Za-z0-9_.-]+)\s*=)").expect("valid pattern")
    })
}

fn section_pattern(key: &str) -> Regex {
    Regex::new(&format!(r"^\s*\[{}\]\s*$", regex::escape(key))).expect("valid pattern")
}

/// Best-effort line number of `key` (inside `section` when given).
pub fn find_line(text: &str, key: &str, section: Option<&str>) -> Option<usize> {
    let mut current = String::new();

    for (number, line) in text.lines().enumerate() {
        let Some(captures) = assignment_pattern().captures(line) else {
            continue;
        };
        if let Some(found) = captures.get(1) {
            current = found.as_str().to_string();
        } else if captures.get(2).map(|found| found.as_str()) == Some(key)
            && section.is_none_or(|section| section == current)
        {
            return Some(number + 1);
        }
    }
    None
}

/// Whether `key` appears as a `[key]` section of its own.
pub fn is_section(text: &str, key: &str) -> bool {
    text.lines().any(|line| section_pattern(key).is_match(line))
}

pub fn located(text: &str, key: &str, section: Option<&str>) -> String {
    match find_line(text, key, section) {
        Some(line) => format!("line {line}: "),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
version = 1

[analysis]
mi_min = 20
min_mi = 20

[targets.frontend]
path = \"frontend\"
";

    #[test]
    fn a_key_is_located_inside_its_section() {
        assert_eq!(find_line(SAMPLE, "min_mi", Some("analysis")), Some(5));
        assert_eq!(find_line(SAMPLE, "path", Some("targets.frontend")), Some(8));
    }

    #[test]
    fn a_section_name_is_not_mistaken_for_a_key() {
        assert_eq!(find_line(SAMPLE, "analysis", Some("analysis")), None);
    }

    #[test]
    fn a_missing_key_has_no_line() {
        assert_eq!(find_line(SAMPLE, "ghost", None), None);
    }

    #[test]
    fn the_first_assignment_anywhere_is_found_without_a_section() {
        assert_eq!(find_line(SAMPLE, "path", None), Some(8));
    }

    #[test]
    fn section_detection_looks_at_whole_lines() {
        assert!(is_section(SAMPLE, "analysis"));
        assert!(!is_section(SAMPLE, "mi_min"));
        assert!(!is_section("analysis = 3\n", "analysis"));
    }

    #[test]
    fn located_prefixes_the_line_when_there_is_one() {
        assert_eq!(located(SAMPLE, "min_mi", Some("analysis")), "line 5: ");
        assert_eq!(located(SAMPLE, "ghost", Some("analysis")), "");
    }
}
