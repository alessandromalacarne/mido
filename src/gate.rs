//! The gate vocabulary: the six gates, their order, and their names.
//!
//! One source — the CLI, the config schema, the report and the gate dispatch
//! all read this list instead of repeating it.

use clap::ValueEnum;

/// The ladder, in the order the gates run.
pub const GATES: [Gate; 6] = [
    Gate::Syntax,
    Gate::Size,
    Gate::Analysis,
    Gate::Tests,
    Gate::Coverage,
    Gate::Mutation,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Gate {
    Syntax,
    Size,
    Analysis,
    Tests,
    Coverage,
    Mutation,
}

impl Gate {
    /// The gate's name, as `.mido.toml`, `--gate` and the reports spell it.
    pub const fn name(self) -> &'static str {
        match self {
            Gate::Syntax => "syntax",
            Gate::Size => "size",
            Gate::Analysis => "analysis",
            Gate::Tests => "tests",
            Gate::Coverage => "coverage",
            Gate::Mutation => "mutation",
        }
    }

    /// The gate a name stands for; `None` for a name no gate owns.
    pub fn from_name(name: &str) -> Option<Self> {
        GATES.into_iter().find(|gate| gate.name() == name)
    }

    /// What to do about a gate that did not pass.
    pub const fn fix_hints(self) -> &'static [&'static str] {
        match self {
            Gate::Syntax => &[
                "fix the diagnostics in the changed files",
                "formatting alone may be auto-fixed: run the formatter in write mode, then re-run",
            ],
            Gate::Size => &[
                "split along a real seam — moving the code into another file does not pass this gate",
            ],
            Gate::Analysis => &[
                "extract the hard-to-hold unit; padding comments to raise MI does not pass this gate",
            ],
            Gate::Tests => {
                &["fix the failing tests; never skip, ignore or loosen an assertion to go green"]
            }
            Gate::Coverage => &["add behavior tests for the uncovered lines of the changed files"],
            Gate::Mutation => {
                &["every survivor needs a real assertion, or a written equivalence justification"]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_round_trips_through_its_gate() {
        for gate in GATES {
            assert_eq!(Gate::from_name(gate.name()), Some(gate));
        }
    }

    #[test]
    fn the_ladder_runs_cheap_gates_before_expensive_ones() {
        let names: Vec<&str> = GATES.iter().map(|gate| gate.name()).collect();

        assert_eq!(
            names,
            vec!["syntax", "size", "analysis", "tests", "coverage", "mutation"]
        );
    }

    #[test]
    fn an_unknown_name_is_no_gate() {
        assert_eq!(Gate::from_name("unknown-gate"), None);
    }

    #[test]
    fn every_gate_has_a_fix_hint() {
        for gate in GATES {
            assert!(
                !gate.fix_hints().is_empty(),
                "{} needs a fix hint",
                gate.name()
            );
        }
    }

    #[test]
    fn the_fix_hints_say_what_they_mean() {
        assert!(Gate::Syntax.fix_hints()[0].contains("fix the diagnostics"));
        assert!(Gate::Size.fix_hints()[0].contains("real seam"));
        assert!(Gate::Analysis.fix_hints()[0].contains("padding comments"));
        assert!(Gate::Tests.fix_hints()[0].contains("never skip"));
        assert!(Gate::Coverage.fix_hints()[0].contains("behavior tests"));
        assert!(Gate::Mutation.fix_hints()[0].contains("equivalence justification"));
    }
}
