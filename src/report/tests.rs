use super::*;
use crate::style::Style;
use crate::test_support::sample_results;

#[test]
fn exit_codes_separate_blocked_from_incomplete() {
    assert_eq!(exit_code(&sample_results()[..1]), 0);
    assert_eq!(exit_code(&sample_results()), 1);
    assert_eq!(
        exit_code(&[GateResult::new(
            "coverage",
            INCOMPLETE,
            "no report",
            Vec::<String>::new()
        )]),
        2
    );
    assert_eq!(
        exit_code(&[GateResult::new(
            "mutation",
            FAIL,
            "40% killed",
            Vec::<String>::new()
        )]),
        1
    );
}

#[test]
fn verdict_names_every_gate_that_did_not_pass() {
    let mut all = sample_results();
    all.push(GateResult::new(
        "mutation",
        INCOMPLETE,
        "timed out",
        Vec::<String>::new(),
    ));

    let verdict = verdict(&all);

    assert!(verdict.starts_with("BLOCKED"));
    assert!(verdict.contains("size=FAIL"));
    assert!(verdict.contains("mutation=INCOMPLETE"));
}

#[test]
fn verdict_is_ship_ready_only_when_every_gate_passes() {
    assert_eq!(verdict(&sample_results()[..1]), "SHIP-READY");
}

#[test]
fn a_waived_gate_does_not_block_the_verdict() {
    let results = vec![
        GateResult::new("tests", PASS, "412 passed, 0 failed", Vec::<String>::new()),
        GateResult::new(
            "mutation",
            SKIPPED,
            "`enabled = false` in .mido.toml",
            Vec::<String>::new(),
        ),
    ];

    assert_eq!(verdict(&results), SHIP_READY);
    assert_eq!(exit_code(&results), 0);
}

#[test]
fn a_waived_gate_is_never_named_as_a_failing_one() {
    let mut results = sample_results();
    results.push(GateResult::new(
        "mutation",
        SKIPPED,
        "`enabled = false` in .mido.toml",
        Vec::<String>::new(),
    ));

    let final_verdict = verdict(&results);

    assert!(final_verdict.contains("size=FAIL"), "{final_verdict}");
    assert!(!final_verdict.contains("mutation"), "{final_verdict}");
    assert_eq!(exit_code(&results), 1);
}

#[test]
fn the_verdict_panel_names_the_gate_the_config_waived() {
    let printed = render_verdict(SHIP_READY, &["mutation"], Style::plain());

    assert!(printed.contains("SHIP-READY"), "{printed}");
    assert!(printed.contains("waived: mutation"), "{printed}");
    assert!(printed.contains("enabled = false"), "{printed}");
}

#[test]
fn a_verdict_without_a_waiver_says_nothing_about_one() {
    assert!(!render_verdict(SHIP_READY, &[], Style::plain()).contains("waived"));
}

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
