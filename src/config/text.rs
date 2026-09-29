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
