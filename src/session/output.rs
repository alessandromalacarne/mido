//! What a run prints: the target table, banners, the verdict, and the report.

use super::Session;
use crate::report::{
    render_banner, render_report_markdown, render_verdict, verdict, BannerContext, GateResult,
    ReportContext,
};
use crate::style::Style;
use crate::targets::Target;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub fn print_targets(
    out: &mut dyn Write,
    repo: &Path,
    targets: &BTreeMap<String, Target>,
    style: Style,
) {
    let rows: Vec<(String, String, &'static str)> = targets
        .iter()
        .map(|(name, target)| (name.clone(), target_row_path(target), target_kind(target)))
        .collect();
    let name_width = rows.iter().map(|row| row.0.len()).max().unwrap_or(0).max(4);
    let path_width = rows.iter().map(|row| row.1.len()).max().unwrap_or(0).max(4);

    let _ = writeln!(out, "targets under {}:", repo.display());
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        style.dim(&format!(
            "  {:<name_width$} {:<path_width$} KIND",
            "NAME", "PATH"
        ))
    );
    for (name, path, kind) in rows {
        let _ = writeln!(out, "  {name:<name_width$} {path:<path_width$} {kind}");
    }
}

/// The path column: the workspace roll-up stands for the repo root.
fn target_row_path(target: &Target) -> String {
    if target.path.is_empty() {
        ".".to_string()
    } else {
        target.path.clone()
    }
}

fn target_kind(target: &Target) -> &'static str {
    if target.path.is_empty() {
        "workspace"
    } else if target.workspace_member {
        "member"
    } else {
        "standalone crate"
    }
}

pub fn markdown_report(
    session: &Session,
    target: &Target,
    files: &[String],
    results: &[GateResult],
) -> String {
    let source_count = files
        .iter()
        .filter(|path| session.lang.is_source(path))
        .count();
    render_report_markdown(
        results,
        &ReportContext {
            target,
            revision: &session.revision,
            dirty: &session.dirty,
            files,
            source_label: session.lang.source_label(),
            source_count,
        },
    )
}

pub fn print_verdict(
    out: &mut dyn Write,
    session: &Session,
    target: &Target,
    results: &[GateResult],
    style: Style,
) {
    let final_verdict = verdict(results);
    let _ = writeln!(out, "{}", render_verdict(&final_verdict, style));
    let _ = writeln!(
        out,
        "{}",
        style.dim(&format!(
            "revision stamp: {} | {}",
            session.revision, session.dirty
        ))
    );
    if !session.as_json {
        return;
    }

    let _ = writeln!(
        out,
        "{}",
        verdict_json(session, target, results, &final_verdict)
    );
}

/// The machine-readable verdict `--json` appends.
fn verdict_json(
    session: &Session,
    target: &Target,
    results: &[GateResult],
    final_verdict: &str,
) -> serde_json::Value {
    let gates: Vec<serde_json::Value> = results
        .iter()
        .map(|result| {
            serde_json::json!({
                "gate": result.name,
                "status": result.status,
                "summary": result.summary,
            })
        })
        .collect();
    serde_json::json!({
        "target": target.name,
        "verdict": final_verdict,
        "revision": session.revision,
        "dirty": session.dirty,
        "gates": gates,
    })
}

/// The banner a target's run opens with.
pub(super) fn target_banner(
    session: &Session,
    target: &Target,
    files: &[String],
    style: Style,
) -> String {
    let source_count = files
        .iter()
        .filter(|path| session.lang.is_source(path))
        .count();
    render_banner(
        target,
        &BannerContext {
            revision: &session.revision,
            dirty: &session.dirty,
            files,
            source_label: session.lang.source_label(),
            source_count,
        },
        style,
    )
}

pub fn write_report(
    report_path: Option<&Path>,
    reports: &[String],
    out: &mut dyn Write,
    style: Style,
) {
    let Some(report_path) = report_path else {
        return;
    };
    if let Some(parent) = report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(report_path, reports.join("\n"));
    let _ = writeln!(
        out,
        "{}",
        style.dim(&format!("report written to {}", report_path.display()))
    );
}
