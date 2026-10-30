use super::*;
use crate::style::Style;

#[test]
fn the_verdict_panel_paints_by_outcome() {
    let shipped = render_verdict(SHIP_READY, &[], Style::colored());
    let blocked = render_verdict("BLOCKED — size=FAIL", &[], Style::colored());

    assert!(
        shipped.contains("\u{1b}[32mSHIP-READY\u{1b}[0m"),
        "{shipped:?}"
    );
    assert!(
        blocked.contains("\u{1b}[31mBLOCKED — size=FAIL\u{1b}[0m"),
        "{blocked:?}"
    );
}

#[test]
fn a_panel_pads_painted_rows_by_their_visible_width() {
    let printed = panel(
        "verdict",
        &[Style::colored().pass("SHIP-READY")],
        Style::colored(),
    );

    assert!(
        printed.contains("│ \u{1b}[32mSHIP-READY\u{1b}[0m │"),
        "the row closes flush with the rule: {printed:?}"
    );
    assert!(
        printed.lines().all(|line| visible_len(line) == 14),
        "{printed:?}"
    );
}

#[test]
fn a_panel_is_never_narrower_than_its_title() {
    let printed = panel("failure", &[], Style::plain());

    assert_eq!(printed, "╭─ failure ╮\n╰──────────╯");
}
