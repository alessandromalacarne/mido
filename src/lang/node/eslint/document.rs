//! What one eslint json document answered: the code lines and block nesting
//! per file, and the function units the size and analysis gates judge.

use crate::metrics::{Measurements, Unit};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::path::Path;

/// The metric rules the node modules inject and read back. Each one is forced
/// to report a reading for every file, block and function it sees — `max: 0`
/// is what makes eslint measure instead of judge, and the reading itself is in
/// the rule's own message.
pub const MAX_LINES: &str = "max-lines";
pub const FUNCTION_LINES: &str = "max-lines-per-function";
pub const COMPLEXITY: &str = "complexity";
pub const NESTING: &str = "max-depth";
/// The one reading eslint core has none of: it comes from the project's own
/// plugin, and a project that does not load it fails the pass loudly rather
/// than measuring nothing.
pub const COGNITIVE: &str = "sonarjs/cognitive-complexity";

/// Read one document into the readings a target's gates judge. A file the
/// document never answered for is an error — a missing reading is never a zero.
pub fn readings(files: &[String], document: &Json) -> Measurements {
    let mut measurements = Measurements {
        files: files.to_vec(),
        ..Measurements::default()
    };
    let mut answered: Vec<String> = Vec::new();

    for report in reports(document) {
        let Some(file) = report
            .get("filePath")
            .and_then(Json::as_str)
            .and_then(|reported| measured_file(files, reported))
        else {
            continue;
        };
        let mut readings = Readings::default();
        for message in messages(report) {
            readings.message(&file, message);
        }
        readings.commit(&file, &mut measurements);
        answered.push(file);
    }

    for file in files
        .iter()
        .filter(|file| !answered.iter().any(|seen| seen == *file))
    {
        measurements
            .errors
            .push(format!("{file}: eslint returned no report"));
    }
    measurements
}

/// One file's messages while they are read: the readings it answered with, and
/// the cognitive readings still to be attributed to their functions.
#[derive(Default)]
struct Readings {
    lines: Option<i64>,
    nesting: Option<i64>,
    functions: BTreeMap<(usize, usize), Unit>,
    cognitive: Vec<(usize, usize, i64)>,
    errors: Vec<String>,
}

impl Readings {
    /// One message: the reading its rule spells, the error a file eslint could
    /// not read carries, or nothing — the project's own lint findings are the
    /// syntax gate's business.
    fn message(&mut self, file: &str, message: &Json) {
        let rule = message.get("ruleId").and_then(Json::as_str);
        let text = message
            .get("message")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let (line, column) = position(message);

        match rule {
            None => self.errors.push(format!("{file}: {text}")),
            Some(MAX_LINES) => {
                self.lines = self.value(file, MAX_LINES, text, paren_value(text));
            }
            Some(NESTING) => {
                self.nesting = self
                    .value(file, NESTING, text, paren_value(text))
                    .max(self.nesting);
            }
            Some(FUNCTION_LINES) => {
                let Some(value) = self.value(file, FUNCTION_LINES, text, paren_value(text)) else {
                    return;
                };
                let unit = self.function(file, line, column);
                unit.sloc = value;
                unit.name = function_name(text);
            }
            Some(COMPLEXITY) => {
                let Some(value) =
                    self.value(file, COMPLEXITY, text, number_after(text, "complexity of "))
                else {
                    return;
                };
                let unit = self.function(file, line, column);
                unit.cyclomatic = value;
                unit.name = function_name(text);
            }
            Some(COGNITIVE) => {
                let Some(value) = self.value(file, COGNITIVE, text, number_after(text, "from "))
                else {
                    return;
                };
                self.cognitive.push((line, column, value));
            }
            Some(_) => {}
        }
    }

    /// The number a metric rule spells, or the error that says the message came
    /// in a shape mido cannot read — never a silent zero.
    fn value(&mut self, file: &str, rule: &str, text: &str, value: Option<i64>) -> Option<i64> {
        match value {
            Some(value) => Some(value),
            None => {
                self.errors.push(format!(
                    "{file}: eslint reported `{rule}` in a shape mido cannot read: {text}"
                ));
                None
            }
        }
    }

    /// The unit a rule reported at `(line, column)`, created on first mention.
    /// A function only one rule reported keeps the other reading's floor.
    fn function(&mut self, file: &str, line: usize, column: usize) -> &mut Unit {
        self.functions
            .entry((line, column))
            .or_insert_with(|| Unit::new(file, ""))
    }

    fn commit(mut self, file: &str, measurements: &mut Measurements) {
        self.attribute_cognitive();
        if let Some(lines) = self.lines {
            measurements.lines.insert(file.to_string(), lines);
        }
        if let Some(nesting) = self.nesting {
            measurements.nesting.insert(file.to_string(), nesting);
        }
        measurements.units.extend(self.functions.into_values());
        measurements.errors.extend(self.errors);
    }

    /// A cognitive reading belongs to the function whose start sits at or
    /// before it: the rule reports inside the function, not at its first line.
    fn attribute_cognitive(&mut self) {
        for (line, column, value) in &self.cognitive {
            let owner = self
                .functions
                .range(..=(*line, *column))
                .next_back()
                .map(|(key, _)| *key);
            if let Some(unit) = owner.and_then(|owner| self.functions.get_mut(&owner)) {
                unit.cognitive = unit.cognitive.max(*value);
            }
        }
    }
}

fn reports(document: &Json) -> &[Json] {
    document.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn messages(report: &Json) -> &[Json] {
    report
        .get("messages")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The measured file a report is about: eslint echoes the path it resolved,
/// and the gates report the run's own spelling of the file.
fn measured_file(files: &[String], reported: &str) -> Option<String> {
    let reported = Path::new(reported);
    files
        .iter()
        .find(|file| reported.ends_with(file.as_str()))
        .cloned()
}

/// Where a message points, 1-based as eslint prints it.
fn position(message: &Json) -> (usize, usize) {
    let value = |key: &str| message.get(key).and_then(Json::as_u64).unwrap_or(0) as usize;
    (value("line"), value("column"))
}

/// The number these rules spell in parentheses: `File has too many lines (17).`
fn paren_value(message: &str) -> Option<i64> {
    let open = message.find('(')?;
    let rest = &message[open + 1..];
    let close = rest.find(')')?;
    rest[..close].trim().parse().ok()
}

/// The number after a marker: `has a complexity of 7.`, `Cognitive Complexity
/// from 12 to the 0 allowed.`
fn number_after(message: &str, marker: &str) -> Option<i64> {
    let start = message.find(marker)? + marker.len();
    message[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// The name a function metric rule prints — `Function 'classify'`, `Method
/// 'save'` — or the kind of function it names when it prints no name, as
/// arrows and anonymous functions come out.
fn function_name(message: &str) -> String {
    if let Some(name) = quoted(message) {
        return name;
    }
    let kind = message.split(" has").next().unwrap_or("Function");
    kind.trim().to_lowercase()
}

fn quoted(message: &str) -> Option<String> {
    let open = message.find('\'')?;
    let rest = &message[open + 1..];
    let close = rest.find('\'')?;
    Some(rest[..close].to_string())
}
