# mido 🌲

> "If you want to pass through here, you should at least equip a sword and shield!"
> — [Mido](https://zelda.fandom.com/wiki/Mido), guarding the way into Kokiri Forest

Like the Kokiri who won't let you into the forest without proof you're ready,
`mido` won't let a change through without proof it holds. It runs the six-gate
verification ladder described by `.mido.toml` against the whole workspace — or
the packages you name — and blocks the way until every gate passes.

## Documentation

The full reference lives in the [wiki](https://github.com/alessandromalacarne/mido/wiki):
getting started, CLI, configuration, targets, gates, reports, the MCP server,
troubleshooting and development.

## Built for LLM agents

mido assumes an LLM wrote the change and an LLM reads the result.

- Non-interactive by contract: argv in, verdict out, exit codes `0`/`1`/`2`.
  Nothing to watch, page through or interpret — an agent branches on the exit
  code, and `--json` gives it structure when prose is not enough.
- The failure report is the handoff artifact: ladder position, the contract the
  gate enforced, the evidence, and fix hints written as instructions — "split
  along a real seam", "never skip, ignore or loosen an assertion". It is meant
  to be the next agent's prompt.
- A verdict is bound to the revision it measured (`HEAD` plus a dirty-state
  hash), so a green run from before the last edit cannot be replayed on it.
- Commands are argv arrays, never shell lines: what `.mido.toml` declares
  is exactly what runs. An agent editing the config cannot smuggle a pipeline
  into a gate.
- Output defaults to where agent sessions already keep their artifacts
  (`$COMMANDCODE_SCRATCHPAD`), and `--json` turns the verdict machine-readable.
- Speaks MCP: `mido mcp` serves the ladder as MCP tools on stdio, so an
  MCP-speaking agent calls it directly instead of shelling out.
- Rendering is honest about who is watching: piped (or with `NO_COLOR`,
  `TERM=dumb`) the run drops colour and the live status line, so what an agent
  captures is a stable, diffable log of the same panels and gate lines.

## Opinionated by design

mido is not a linter framework. The ladder is the opinion, and it does not bend.

- Six gates, one order, one vocabulary: `PASS`, `FAIL`, `INCOMPLETE`, `SKIPPED`,
  `SHIP-READY`, `BLOCKED`. No custom gates, no plugins.
- Thresholds ship with defaults; `.mido.toml` can move them, but unknown
  keys are config errors — the ladder refuses to guess what a typo meant.
- Waivers have to be written down: `enabled = false` stops a gate from running
  and still reports `SKIPPED`, and a skipped gate is never a pass.
- Fix hints prescribe the honest fix: moving code into another file does not
  pass the size gate, padding comments do not raise the maintainability index,
  and a surviving mutant needs a real assertion or a written equivalence
  argument.

## A baseline for AI-built projects

If an agent wrote the code, something has to prove the code holds. mido enforces
the floor, in cost order: it compiles and passes the formatter, linter and type
checker; it is small enough to read; it is tested; the tests cover the code;
and the tests actually assert — mutants die. That is the minimum a
project built with LLMs should not ship below. Start by copying this repo's
`.mido.toml`, and run `mido` before every handoff.

## What it does

- Runs the gates in a fixed order: **syntax → size → analysis → tests → coverage → mutation**.
- Drives one **language module** at a time — inferred from the repo (a root `Cargo.toml`
  selects `rust`) or forced with `--lang`. The module owns the default commands, the
  output parsers, target detection, the workspace aid and the embedded baseline
  contract; a repo's `.mido.toml` overrides that baseline key by key. A repo no
  module recognizes exits 2 and says so.
- Measures the **whole workspace** — no diff is read. `-p NAME` (repeatable,
  cargo package semantics: the name the manifest declares) narrows the run to
  the packages you name, each measured whole: the workspace roll-up is what
  runs without one.
- Stamps the verdict on the exact revision it measured (`HEAD` plus a dirty-state
  hash), so a green verdict cannot be reused on changed code.
- Prints a failure report meant for the next reader (human or agent): gate
  position in the ladder, the contract it enforced, the evidence, and fix hints.
- Refuses to treat "did not run" as "passed": a gate missing its tooling is
  `INCOMPLETE`, a gate turned off in the config is `SKIPPED`, and neither is ever
  a green verdict.

## The six gates

| # | Gate | What it enforces | Tooling |
|---|------|------------------|---------|
| 1 | `syntax` | formatter, linter, type checker pass on the measured target | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo check` (argv configurable) |
| 2 | `size` | file LOC, function LOC, cyclomatic complexity, nesting depth | `tokei` + `rust-code-analysis` |
| 3 | `analysis` | maintainability index floor, cognitive complexity ceiling | `rust-code-analysis` |
| 4 | `tests` | the test command passes | `cargo test` (argv configurable) |
| 5 | `coverage` | total line coverage floor, coverage drop vs a baseline | `cargo llvm-cov` (lcov) |
| 6 | `mutation` | percentage of mutants killed in the package | `cargo-mutants` |

Built-in defaults, all overridable in `.mido.toml`:

- **size** — file: 300 warn / 500 fail; function: 40/60; complexity: 10/15; nesting: 3/4
- **analysis** — maintainability index ≥ 20; cognitive complexity ≤ 15
- **coverage** — total ≥ 80%; drop ≤ 0 points
- **mutation** — kill rate ≥ 70%
- **timeouts** — syntax 1800s, tests 900s, mutation 3600s

## Verdicts and exit codes

| Verdict | Exit | Meaning |
|---------|------|---------|
| `SHIP-READY` | 0 | every selected gate passed |
| `BLOCKED — gate=FAIL, …` | 1 | at least one gate failed |
| `BLOCKED — gate=INCOMPLETE, …` | 2 | a gate could not run to a verdict (missing tool, unanalyzable file, timeout) |
| `BLOCKED — gate=SKIPPED, …` | 2 | a gate was disabled in the config, or nothing was measured |

A `SKIPPED` gate is not a passed gate: `0` is reserved for gates that actually
ran and passed. `INCOMPLETE` is never a pass either — fix the tooling (usually
`nix develop`) and re-run before handing the work off.

## Reading the output

A run has a shape: the banner opens it, one line per gate walks the ladder, the
verdict panel closes it.

```
╭─ mido ──────────────────────────────────────────────╮
│ target   workspace (./)                             │
│ revision febf29d1                                   │
│ dirty    bddc06f7                                   │
│ measured 14 files (14 rust)                         │
╰─────────────────────────────────────────────────────╯
  Cargo.toml
  src/style.rs

  [1/1] ✓ syntax  clean
      format: 0 file(s) with diffs
      lint: 0 diagnostic(s)
      typecheck: 0 diagnostic(s)

╭─ verdict ──╮
│ SHIP-READY │
╰────────────╯
revision stamp: febf29d11eec8ddb2bdea1f5fb1d01524543dc34 | bddc06f72e892e557a404e502473cd371127778a
```

- A gate line carries its place in the selection, a status glyph, the gate name
  in a fixed column and the one-line summary: `✓` PASS, `✗` FAIL, `!` INCOMPLETE,
  `·` SKIPPED. The glyph is the fallback channel — colour never carries the
  verdict alone.
- While a slow gate runs, a terminal sees a `[place] ⋯ name  running…` line under
  the ladder, rewritten in place when the answer lands: `cargo test` takes a
  minute, and a frozen screen should not read as a hung one.
- Colour: green `PASS`, red `FAIL`, yellow `INCOMPLETE`, dim `SKIPPED`. A piped
  run paints nothing and never writes the live line, and `NO_COLOR` (non-empty)
  or `TERM=dumb` switch colour off by hand.
- The banner, the verdict and the failure report are boxed panels. Hashes in a
  panel are shortened to eight characters so the eye can compare them; the
  revision stamp under the verdict and the markdown report keep the full ones,
  because a verdict is bound to the revision it measured.
- Errors lead with a red `error:` and step their details and hint back to dim;
  warnings lead with a yellow `warning:`. `--list-targets` prints an aligned
  `NAME PATH KIND` table.

## Install

With Nix (the dev shell brings every gate tool along):

```sh
nix build          # the binary
nix develop        # cargo, rustfmt, clippy, tokei, rust-code-analysis, cargo-llvm-cov, cargo-mutants, cargo-nextest
```

With cargo:

```sh
cargo build --release
```

When `cargo` is not on `PATH`, every gate command is run through `nix develop -c …`
so the dev shell provides the tools; otherwise commands are spawned directly.

## Default tool requirements

The ladder shells out: mido decides, the tools measure. The Rust module's
built-in baseline (`src/lang/rust/defaults.toml`) expects these on `PATH`:

| Gate | Tools the baseline calls | Where they come from |
|------|--------------------------|----------------------|
| `syntax` | `cargo fmt`, `cargo clippy`, `cargo check` | the Rust toolchain, with the `rustfmt` and `clippy` components |
| `size` | `tokei`, `rust-code-analysis` | tokei reads file LOC; rust-code-analysis reads function metrics |
| `analysis` | `rust-code-analysis` | maintainability index and cognitive complexity |
| `tests` | `cargo test` | the Rust toolchain, or whatever `[tests] command` names |
| `coverage` | `cargo llvm-cov` + LLVM `llvm-cov`/`llvm-profdata` | `cargo install cargo-llvm-cov`; LLVM tools from the `llvm-tools-preview` component, or system LLVM via `LLVM_COV`/`LLVM_PROFDATA` |
| `mutation` | `cargo mutants` | `cargo install cargo-mutants`; add `cargo-nextest` for the recommended `test_tool = "nextest"` setup |

Every command is an argv array in `.mido.toml`, so pointing a gate at a
different tool is a config edit, not a code change. A missing tool never passes
a gate: the command cannot run, the gate reports `INCOMPLETE`, and the run
exits `2`. Install the tool — or work inside `nix develop`, whose dev shell
carries all of them.

## Usage

```
mido [-p NAME]… [--lang LANG] [--repo PATH] [--gate GATE]…
     [--list-targets] [--apply-workspace-aid]
     [--baseline-lcov PATH] [--report PATH] [--json]
mido mcp
```

```sh
mido                          # measure the whole workspace
mido -p lib                   # measure one package in full
mido -p lib -p cli            # … several packages, in turn
mido --lang rust              # force the language module; default: inferred from the repo
mido --list-targets           # show what can be measured, then exit
mido --gate coverage --gate mutation   # run a subset of the ladder
mido --json                   # machine-readable verdict
mido --report out.md          # write the markdown report
mido --baseline-lcov base.info         # coverage delta against a previous lcov
mido mcp                      # serve the ladder as MCP tools on stdio
```

`-p` resolves the way cargo resolves `--package`: by the name the target's
manifest declares, with a declared `[targets.<name>]` name accepted too. An
unknown name is a setup error (exit 2) — never a silent whole-workspace run.

## MCP server

`mido mcp` serves the ladder as MCP tools on stdio, so an MCP-speaking agent
calls it directly instead of shelling out. Point a client at it with:

```json
{
  "mcpServers": {
    "mido": {
      "command": "mido",
      "args": ["mcp"]
    }
  }
}
```

Two tools, each the CLI surface for one job:

- `list_targets` — name, path and kind of every target the repo offers
  (`mido --list-targets`).
- `run_ladder` — measures one target. The tool result is the run's report,
  prefixed with `exit_code`; the tool is marked `isError` when the code is not
  `0` — `1` BLOCKED (the failure report inside is the handoff), `2` INCOMPLETE
  or a setup error. `tests` and `mutation` run for as long as the CLI takes.

Every argument maps to a flag (`packages` maps to `-p`, alongside `repo`,
`lang`, `gates`, `apply_workspace_aid`, `baseline_lcov`, `report`, `json`), so
the CLI's rules — an unknown package is a setup error, and so is anything the
server does not know — apply unchanged; a typo in an argument name is rejected,
not dropped. stdout carries protocol messages only; anything the server itself
has to say goes to stderr.

## Workspace and packages

Every gate measures the whole target; no diff is read. Without `-p`, that target
is the **workspace** roll-up: the crate at the repo root, or the workspace those
members belong to, minus the crates the root manifest excludes. The measured set
is every file git can see — tracked files plus untracked ones that are not
ignored — scoped to the target: size, syntax and analysis measure them all, the
tests gate runs the target's commands, and the coverage and mutation gates run
whole-package tools. A repository git lists no file for measures nothing and
exits 2.

`-p NAME` picks packages instead, resolved the way cargo resolves `--package`:
the name the target's manifest declares, plus declared `[targets.<name>]` names.
Repeat the flag to measure several in turn; a name no target answers to is a
setup error (exit 2) — never a silent whole-workspace run. `--list-targets`
prints the names.

## Targets

`mido` measures one target at a time:

- **workspace** — the crate (or workspace root) at the repo root;
- **member** — each crate listed in the root `[workspace] members`;
- **standalone crate** — a top-level crate directory excluded from the workspace;
- **declared** — any `[targets.<name>]` section in `.mido.toml`.

The banner and the report name the target and the files it measured. A
standalone crate is its own target: the workspace roll-up never takes its files,
and its gate commands do not inherit the root `[gate]` sections.

## Configuration

`.mido.toml` is the tool contract. Rust also ships a built-in baseline — the
same contract, embedded as `src/lang/rust/defaults.toml` — so a
cargo project with no config still gets the commands and thresholds below. The
file overrides the baseline key by key; standalone (excluded) crates never
inherit root commands and fall back to the module's bare `cargo` commands
instead.

Example, mirroring this repo's own:

```toml
version = 1

[syntax]
format    = ["cargo", "fmt", "--check"]
lint      = ["cargo", "clippy", "--all-targets", "--all-features", "--", "-D", "warnings"]
typecheck = ["cargo", "check"]

[size]
tool         = "tokei"                          # tokei is the supported tool
file_loc     = { warn = 300, fail = 500 }       # non-blank, non-comment lines per file
function_loc = { warn = 40,  fail = 60  }       # lines per function
complexity   = { warn = 10,  fail = 15  }       # cyclomatic or cognitive, per function
nesting      = { warn = 3,   fail = 4   }       # deepest block nesting

[analysis]
tool          = "rust-code-analysis"            # rust-code-analysis is the supported tool
mi_min        = 20                              # maintainability index floor (0–100)
cognitive_max = 15                              # cognitive complexity, per function

[tests]
command      = ["cargo", "test", "--all-features"]
timeout_secs = 900

[coverage]
command          = ["cargo", "llvm-cov", "--all-features", "--lcov", "--output-path", "lcov.info"]
coverage_min     = 80   # percent total line coverage of the measured target
total_drop_max   = 0    # percentage points of total coverage you tolerate losing

[mutation]
command       = ["cargo", "mutants", "-j2"]
timeout_secs  = 3600
kill_rate_min = 70             # percent of mutants killed

[failure]
max_attempts_per_gate = 3      # distinct hypotheses the report allows per gate

[targets.frontend]             # optional per-target overrides
path     = "frontend"          # directory holding the crate
manifest = "frontend/Cargo.toml"
```

Notes:

- `version = 1` is required; without it the run warns.
- Every command is an **argv array**, never a shell line: `["cargo", "test"]`,
  not `"cargo test | tail"`. A string command is a config error with the
  migration example in the hint.
- `[targets.<name>.<gate>]` overrides the root `[gate]` section for that target.
  Root commands are inherited by workspace members only — a standalone crate
  (one the root manifest excludes) does not inherit them, so `--all-features`
  written for the workspace root does not break a wasm crate.
- The mutation gate drives **every test command the target declares**: a native
  logic suite and a browser suite each get their turn at every mutant, with the
  arguments that command names. Passes after the first run with `--iterate`, so
  the mutants an earlier pass caught come back as skipped — the evidence names
  each pass and what it caught.
- The mutation gate **owns cargo-mutants' output directory**: every run gets its
  own `--output` under the run scratch, so a verdict can only ever be decided by
  the mutants that run tested — state from an earlier run (`mutants.out`) is
  never read. A `--output` on the configured command is a conflict, not an
  override: the gate reports INCOMPLETE. The counts come from the run's
  `outcomes.json`; mutants that produced no verdict (timed out, or otherwise
  unclassified) are INCOMPLETE, never silently dropped from the kill rate.
- Unknown keys, wrong types and malformed commands are config errors on purpose,
  so a typo cannot silently disable a gate or move a threshold.
- For mutation speed, the recommended setup (shipped by this repo) is a
  `[profile.mutants]` inheriting `test` with `debug = "none"` in `Cargo.toml`
  and a `.cargo/mutants.toml` with `test_tool = "nextest"` and
  `profile = "mutants"`, with `-j2` on the command: the mutants run two at a
  time. Every timed run is a fresh sweep of the package — the gate never
  reuses a previous run's kills. When a sweep is slow, cargo-mutants'
  performance guide applies directly: <https://mutants.rs/performance.html>.

## Reports

Every measured target closes with the verdict panel and the full revision stamp,
and the run writes the markdown report (passing or blocked) and prints where it
landed. A blocked run prints the failure report first — the artifact meant to be
handed to the next attempt:

```
╭─ failure ───────────────╮
│ BLOCKED — size=FAIL     │
│ target   workspace (./) │
│ revision febf29d1       │
│ dirty    b1ab9f36       │
│ failing  1 of 1 gates   │
╰─────────────────────────╯

  [2/6] ✗ size — FAIL
      summary   worst function 74 sloc / cc 1 (oversized_demo)
      contract  `.mido.toml` [size] file_loc.fail=500, function_loc.fail=60, complexity.fail=15, nesting.fail=4
      evidence
        - file src/size_demo.rs: 74 code lines (ok)
        - nesting: not measured (rust-code-analysis exposes no nesting metric for this input)
        - src/size_demo.rs: oversized_demo has function_loc 74 (fail >= 60)
      fix
        - split along a real seam — moving the code into another file does not pass this gate
```

Each blocked gate carries its position in the ladder, the contract it enforced,
the evidence, and the fix hints; a gate that passed is not restated.

- `--report PATH` — explicit location; a relative path is resolved against the repo root.
- Without `--report`, the report goes to `$COMMANDCODE_SCRATCHPAD/guardrails-report.md`
  when the session sets that variable; otherwise only stdout carries it.
- `--json` — the verdict as JSON, with per-gate statuses, for callers that parse.
- Gate artifacts (coverage lcov, mutation output) live under
  `$COMMANDCODE_SCRATCHPAD/guardrails/` or `~/.cache/guardrails/`.

## Workspace aid

In a worktree nested under `.agents/`, cargo walks up into the main checkout and
cannot resolve the crate's own workspace root. `mido` probes with
`cargo locate-project --workspace`; when the answer is wrong, the run stops with
a setup error pointing at `--apply-workspace-aid`. That flag prepends a
`[workspace]` marker to the target's `Cargo.toml` and marks the file
`git update-index --skip-worktree`, so the aid is local-only and never committed.

## Development

```sh
nix develop
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

`mido` is verified with its own ladder: this repo carries a `.mido.toml`
and the six gates are run against the workspace before handoff.

## Layout

| Module | Role |
|--------|------|
| `src/cli.rs` | the argv surface and exit codes |
| `src/gate.rs` | the gate vocabulary: names, order, fix hints — the one gate list |
| `src/mcp.rs` | the MCP server: `mido mcp` speaks JSON-RPC on stdio, with the tool schemas and argv mapping in `src/mcp/` |
| `src/lang/` | language modules: the rust module's embedded defaults, parsers, targets and workspace aid |
| `src/session.rs` | one run: what is measured, in what order, what is printed (`session/{setup,output}.rs`) |
| `src/targets.rs` | target scoping and the measured file set |
| `src/config/` | `.mido.toml` loading, validation and defaults |
| `src/gates/` | the six gates and their reporting |
| `src/report.rs` | verdicts, gate lines, panels, failure report, markdown report (`report/{panel,views,markdown}.rs`) |
| `src/style.rs` | the styling vocabulary: colour, glyphs, terminal detection |
| `src/process.rs` | process execution, git queries, `nix develop` fallback (`process/git.rs`) |

## License

MIT — see [LICENSE](LICENSE).
