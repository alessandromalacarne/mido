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
                "fix the diagnostics the formatter, linter and type checker report",
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
            Gate::Coverage => &["add behavior tests for the lines the report shows uncovered"],
            Gate::Mutation => {
                &["every survivor needs a real assertion, or a written equivalence justification"]
            }
        }
    }
}

