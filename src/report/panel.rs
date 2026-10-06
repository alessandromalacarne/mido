//! The drawing primitives: boxes, fields, and the lines gates report on.

use super::{GateResult, FAIL, INCOMPLETE, PASS, SHIP_READY, SKIPPED};
use crate::style::Style;

/// The glyph that stands for a status, so an uncoloured log still reads.
pub fn status_glyph(status: &str) -> &'static str {
    match status {
        PASS => "✓",
        FAIL => "✗",
        INCOMPLETE => "!",
        SKIPPED => "·",
        _ => "?",
    }
}

/// One gate's line: `  [2/6] ✗ size      2 functions over the ceiling`.
pub fn gate_line(
    position: usize,
    total: usize,
    width: usize,
    result: &GateResult,
    style: Style,
) -> String {
    let head = format!(
        "{} {:<width$}",
        status_glyph(&result.status),
        result.name,
        width = width
    );
    format!(
        "  [{position}/{total}] {}  {}",
        style.paint_status(&result.status, &head),
        result.summary
    )
}

/// The line a gate runs under until it answers — rewritten in place.
pub fn gate_progress_line(
    position: usize,
    total: usize,
    width: usize,
    gate: &str,
    style: Style,
) -> String {
    style.dim(&format!(
        "  [{position}/{total}] ⋯ {gate:<width$}  running…",
        width = width
    ))
}

/// The width a terminal gives the text — escape sequences take no columns, so
/// a painted row pads exactly like a plain one.
pub fn visible_len(text: &str) -> usize {
    let mut width = 0;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            for escape in characters.by_ref() {
                if escape == 'm' {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

/// A labelled box the eye can land on: title on the top rule, one padded row
/// per fact.
pub fn panel(title: &str, rows: &[String], style: Style) -> String {
    let widest = rows.iter().map(|row| visible_len(row)).max().unwrap_or(0);
    let inner = widest.max(title.len() + 1);
    let mut lines = vec![format!(
        "╭─ {} {}╮",
        style.bold(title),
        "─".repeat(inner - title.len() - 1)
    )];
    lines.extend(
        rows.iter()
            .map(|row| format!("│ {row}{} │", " ".repeat(inner - visible_len(row)))),
    );
    lines.push(format!("╰{}╯", "─".repeat(inner + 2)));
    lines.join("\n")
}

/// The verdict, painted by what it says, inside the panel that closes a run.
pub fn render_verdict(final_verdict: &str, style: Style) -> String {
    let painted = if final_verdict == SHIP_READY {
        style.pass(final_verdict)
    } else {
        style.fail(final_verdict)
    };
    panel("verdict", &[painted], style)
}

/// A hash the eye can compare at a glance; the report keeps the full one.
pub(super) fn short(hash: &str) -> &str {
    match hash.char_indices().nth(8) {
        Some((index, _)) => &hash[..index],
        None => hash,
    }
}

/// The label column of the banner and the failure header.
pub(super) fn field(label: &str, value: &str) -> String {
    format!("{label:<8} {value}")
}
