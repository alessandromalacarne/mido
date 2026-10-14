# Changelog

Notable changes to `mido-guard` — the `mido` binary. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-10-14

Every gate now measures the whole target — the workspace roll-up, or the
packages `-p` names — and the language seam grows the npm ecosystem: a root
`package.json` runs the javascript module, a root `tsconfig.json` the
typescript one.

### Added

- **JavaScript language module** — inferred from a root `package.json`, or
  forced with `--lang javascript`. Syntax runs `npx --no-install prettier
  --check` and `eslint` (no typecheck: plain javascript has none); the tests
  gate runs the package manager's `test` script, picked from the lockfile
  (`pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`, `package-lock.json` — npm when
  there is none, target directory before repo root); mutation drives `stryker`.
  Targets follow npm semantics: `workspaces` patterns (`*` one segment, `**`
  any, `!` negation) become members, a negated or unpatterned top-level package
  is its own target, and `node_modules` and dot directories are never walked.
  Measures `.js`, `.mjs`, `.cjs`, `.jsx`.
- **TypeScript language module** — inferred from a root `tsconfig.json`, or
  forced with `--lang typescript`. The javascript baseline plus `tsc --noEmit`
  in the syntax gate, over `.ts` and `.tsx`. `.mts`/`.cts` are deliberately not
  measured: the metrics tool parses neither, and a measured file it cannot read
  would keep `size` and `analysis` INCOMPLETE forever.
- **Coverage derived from the repository** for the npm modules: the target's
  devDependencies (or the hoisted root's) pick vitest's or jest's own coverage
  when one is named, else `c8` wraps the test script. Every derived command
  writes under `coverage/`, and the gate reads the report directory back from
  the command's own flags (`--coverage.reportsDirectory`, `--coverageDirectory`,
  `--reports-dir`, `--report-dir`).
- **`-p NAME` / `--package NAME`** (repeatable) — measure only the packages you
  name, resolved the way the target's manifest spells them (the cargo package,
  the npm package, or a declared `[targets.<name>]` name). An unknown name is a
  setup error (exit 2), never a silent whole-workspace run. Without `-p`, the
  whole workspace roll-up is measured.
- **`--silent`** — stdout carries nothing: the exit code is the verdict, the
  report is the evidence, and stderr keeps its errors and warnings.
- **A bare `--report`** writes the repository's own `.mido/report.md`, next to
  `.mido.toml`.
- **MCP**: `run_ladder` gains `packages` (array → repeated `-p`), `silent`
  (default `true`) and `report` (default the repository's `.mido/report.md`), so
  a tool result is `exit_code` plus stderr with the markdown report as the
  artifact, instead of a printed ladder filling the agent's context.
- **CI**: `.github/workflows/ci.yml` runs `cargo clippy --all-targets
  --all-features -- -D warnings` and `cargo test --all-features` on pushes and
  pull requests.
- `nodejs` in the dev shell — the npm modules' gates run through `npx`.

### Changed

- **The mutation gate runs every declared test command.** One pass per command,
  later passes with `--iterate` so earlier catches come back as skipped; the
  union of the run's counts is the verdict. A target whose behavior lives in a
  second suite no longer has unfalsifiable mutants there.
- **The mutation gate owns the tool's state.** Every run gets its own `--output`
  directory under the run scratch, cleared before the first pass, and the counts
  come from the run's `outcomes.json` — a previous run's report is never read.
  A `--output` on the configured command is a conflict, not an override. A
  mutant with no verdict (timed out, otherwise unclassified) is INCOMPLETE;
  survivors and timed-out mutants are named. The shipped default command drops
  `--iterate`.
- **Mutation is scoped to the change** through `in_diff = true` (default): mido
  generates the diff with git — merge base with the default branch up to the
  working tree, untracked sources included — and every pass runs
  `cargo-mutants --in-diff <file>`. A change touching no rust file of the target
  passes with that stated as its evidence; a change whose lines yield no mutant
  is INCOMPLETE. `in_diff = false` restores the whole-package sweep. The node
  baselines ship `in_diff = false`, because stryker has no `--in-diff`; asking
  for `in_diff = true` there is refused with that reason.
- **Stryker runs from a config mido writes** into the run's own directory
  (extending the repository's `stryker.config.*` when present) and passes
  positionally, with the json report pointed inside that directory: the report
  the verdict reads can only be one this run wrote. `Ignored` is the one
  exclusion; `Killed` is a kill, `Survived` and `NoCoverage` are survivors,
  `CompileError` is unviable, and `Timeout` or an unknown status is INCOMPLETE.
- **Every gate is judged against the whole target.** Syntax fails on every
  diagnostic the steps report — no changed-file filter, no crate-wide-debt
  summary — and a step that failed with nothing attributable is still a problem,
  never a pass. Size and analysis judge the whole file set. Coverage enforces a
  total line floor and keeps the baseline delta; an empty report is INCOMPLETE,
  because nothing measured is unverified, not 100%.
- **Reports drop the diff scope.** The banner and the markdown report lose the
  scope and base rows and count "measured" files; the revision stamp and the
  dirty-state hash are untouched, so every verdict stays bound to the revision
  it measured.

### Removed — breaking

- **CLI**: the positional target, `--base`, `--path`, `--no-diff` and `--all`
  are gone; `-p`/`--package` replaces them, and the diff no longer decides what
  the gates see. The old flags are usage errors, never silent no-ops.
- **Config**: `[coverage] changed_file_min` becomes `coverage_min` (percent
  total line coverage of the measured target), `[mutation] scope` becomes
  `in_diff`, and `[targets.<name>]` loses `scope`. Unknown keys are config
  errors by design, so a stale key fails loudly instead of quietly meaning
  something else.
- **MCP**: `run_ladder` loses `target`, `base`, `paths`, `no_diff` and `all`;
  an argument the server does not know is rejected, not ignored.

## [0.2.0] - 2026-10-07

- Whole-target scope behind `--no-diff`, with `--path` narrowing it and
  `--base` the only conflict; the scope is served over MCP as `no_diff`.
- Whole-target runs pass no mutation scope arguments — the package at the run
  directory is the scope.
- Fixed: mutation patches are written workspace-root relative and scoped with a
  pathspec, so a member target's `--in-diff` paths match the mutants
  cargo-mutants resolves them against.
- Fixed: the binary target stays named `mido` after the package rename to
  `mido-guard`.

## [0.1.0] - 2026-10-06

First published release: the six-gate ladder (`syntax → size → analysis → tests
→ coverage → mutation`) driven from the CLI and configured by `.mido.toml`, the
rust language module behind the language seam, the MCP server on stdio, argv
commands instead of shell lines, verdicts bound to the measured revision, and
the boxed panel rendering.
