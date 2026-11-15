# AGENTS.md — working in mido

`mido` is the guardrails runner: one Rust crate (library + `mido` binary) that
runs the six-gate verification ladder described by `.mido.toml` against the
whole workspace of a repo, or the packages `-p` names. The crate dogfoods its
own ladder: `.mido.toml` in this repo
is the real config, and the runner is `src/main.rs`.

## Hard rules (the product's contract)

These override any global preference or user request. If a rule conflicts with
a user request, follow the rule and explain why.

- A skipped gate is not a passed gate: `enabled = false` is a waiver the ladder
  reports as `SKIPPED`, runs nothing for, and names wherever the verdict is
  printed — that exit code is the configured ladder's, never a claim about the
  gate left out. `INCOMPLETE` is never a pass.
- Unknown config keys, wrong types and malformed commands are config errors —
  a typo must never silently disable a gate or move a threshold.
- Gates never fall back silently: an unsupported tool is `INCOMPLETE`, and the
  dispatch match covers every gate by construction.
- Fix forward. A failing gate means the code is wrong, not the gate — never
  weaken a check, threshold or assertion to make a run green.
- Commands are argv arrays, never shell strings — prevents injection and makes
  argument boundaries unambiguous.
- Keep changes minimal and tightly scoped. Make the smallest useful diff. Don't
  add features, refactor unrelated code, or create abstractions unless explicitly
  asked.
- Don't add fallbacks or silent recovery unless explicitly asked. Fix root
  causes; make code either succeed or fail with a clear error.
- Don't revert unrelated changes. Uncommitted changes are the user's review state.
- If unsure, inspect the codebase instead of inventing patterns.

## Commands

- `cargo test --all-features` — the full suite (unit tests plus `tests/cli.rs` and `tests/mcp.rs`).
- `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` — what the syntax gate checks.
- `cargo run -- --list-targets` / `cargo run -- --gate size` — run the ladder against this repo.
- `nix develop` — the dev shell with every gate tool (`tokei`, `rust-code-analysis`, `cargo-llvm-cov`, `cargo-mutants`, `cargo-nextest`, `nodejs`). When `cargo` is not on `PATH`, gate commands are wrapped in `nix develop -c …` automatically.
- Mutation runs as `cargo mutants -j2` with `.cargo/mutants.toml` (nextest, `profile = "mutants"`) and `[profile.mutants]` in `Cargo.toml`. By default the sweep is scoped to the change: mido writes a git diff (the working tree against `HEAD`, untracked sources included) and passes `--in-diff`; `in_diff = false` in `[mutation]` sweeps the whole package. The gate gives every run its own `--output` directory under the run scratch, so a verdict can only ever be decided by the mutants that run tested — a previous run's `mutants.out` (or an `--iterate` on the configured command) is never read. It drives every test command the target declares, one pass each, later passes with `--iterate`: the in-run union means skipped counts as killed, and the evidence says so. Counts come from the run's `outcomes.json`; a mutant with no verdict (timed out) is INCOMPLETE.

## Architecture (brief)

```
entry:      main.rs ─▶ cli.rs / mcp.rs ─▶ session.rs
gates:      gates/* ─▶ {config, lang, metrics, process, report, targets, style}
lang seam:  lang/mod.rs ─▶ lang/{rust,typescript,javascript}/*
            rust: rust/{tokei,code_analysis,metrics} · node modules: node/eslint · shared: lang/node
support:    config/ · process.rs · metrics.rs · targets.rs · report.rs · style.rs
vocabulary: gate.rs (leaf — the six gates live here and only here)
```

- `src/gate.rs` — the gate vocabulary: the six gates, their order, names and fix
  hints. The CLI (`--gate`), the config schema, the report and the dispatch all
  derive from it; do not reintroduce a second gate list.
- `src/lang/` — the language seam. Everything stack-specific lives in a module
  under it. The ladder itself stays language-generic; new stack behavior belongs
  in the modules, not in `gates/`.
- `src/gates/` — gate judgement and verdicts. Each gate takes a `GateRun` and
  returns a `GateResult`; dispatch in `gates/mod.rs` is exhaustive over `Gate`.
- `src/session.rs` (+ `session/setup.rs`, `session/output.rs`) — one run: target
  selection, file scoping, measurement, printing.
- `src/config/` — `.mido.toml` loading, strict validation, and the layered
  access rule: module defaults ← repo file ← `[targets.<name>]`.
- `src/report.rs` (+ `report/panel.rs`, `report/views.rs`, `report/markdown.rs`)
  — verdicts, gate lines, panels, failure report, markdown report.
- `src/process.rs` (+ `process/git.rs`) — subprocess execution and git queries.
- `src/metrics.rs` — `Unit` (function metrics), `Measurements` (one pass's
  readings: lines, nesting, units, errors) and lcov readings.
- `src/targets.rs` — target scoping and the measured file set.
- `src/mcp.rs` + `src/mcp/` — the MCP server; reuses the CLI in-process.

## Conventions

- Tests live in sibling `tests.rs` files (`src/gates/size.rs` →
  `src/gates/size/tests.rs`), not inline. Shared fixtures live in
  `src/test_support.rs`.
- Gates are tested through `FakeRunner` responses, not real tools; the tool
  summaries (`cargo test`, `cargo mutants`, tokei, …) are the fixtures.
- Comments explain why, not what. Public items carry doc comments — this applies
  to this file too.
- Unused surface is deleted, not kept "just in case" — this is a 0.1.0.

## Workflow

Anything bigger than a one-line chore goes through the repo workflow:

1. **Worktree** — `git worktree add .agents/worktrees/<branch> -b <branch> master`
   (the directory is gitignored). Do all edits, tests and checks in the worktree.
2. **TDD** — red → green → refactor. New behavior gets a failing test first;
   behavior-preserving refactors are pinned by the existing suite.
3. **Guardrails** — run the ladder on the final revision: use the `mido` MCP
   tools (`list_targets`, then `run_ladder`) or `/guardrails`. The verdict is
   bound to `HEAD` + dirty hash; if any tracked file changed after the run, the
   verdict is stale and the ladder re-runs. `BLOCKED`/`INCOMPLETE` stops the
   pipeline — say so loudly, never carry it into a walkthrough or a PR.
4. **Walkthrough** — `/walkthrough` writes `WALKTHROUGH.md`, a review artifact
   that is not committed.
5. **Ship** — `/lgtm`: atomic conventional commits (`feat:`, `fix:`, `refactor:`,
   `test:`, `docs:`, `chore:` — one logical change each) and a PR. Never commit
   or push unless asked.

## Cheat sheet

- Ladder order: `syntax → size → analysis → tests → coverage → mutation`.
- Verdicts: `PASS`, `FAIL`, `INCOMPLETE`, `SKIPPED`; exit codes `0` SHIP-READY,
  `1` FAIL, `2` INCOMPLETE or a setup error.
- Mutation is scoped to the change by default: `--in-diff` on a `git diff HEAD`
  mido writes (untracked sources included), so committed work is out of scope;
  `in_diff = false` in `[mutation]` sweeps the whole package.
- Size ceilings (`.mido.toml`): file 300 warn / 500 fail code lines, function
  40/60 sloc, complexity 10/15, nesting 3/4 (per file in the node modules, where
  eslint's `max-depth` counts blocks). Keep new modules under the warn band and
  functions under 40 sloc; the ladder reports both.