---
name: guardrails
description: Run the post-implementation verification ladder — syntax guards (format/lint/typecheck), module size limits, static analysis (maintainability index, cognitive complexity), tests, coverage, then mutation testing — in that order, fixing forward and never weakening a check to pass. Tool and threshold selection comes from the project's `.mido.toml`, the contract read by `mido`; in this repo `mido` implements the whole ladder, so it is invoked instead of driving gates by hand. Mandatory before /walkthrough or /lgtm — those steps refuse to run without a SHIP-READY verdict for the current revision. Use when implementation is done, or the user says "/guardrails", "run guardrails", "verify this change", "is this done", "run the checks", "set up guardrails", or asks whether work is ready to ship.
argument-hint: "[changed-files-glob] [--gate <name>]"
---

# Guardrails Skill

Post-implementation verification for a finished change. Six gates, strict
order, no skipping:

```
1. syntax guards   → fmt, lint, typecheck, compile          (fast, cheap)
2. module size     → file/function LOC, complexity, nesting
3. analysis        → MI, cognitive complexity
4. tests           → the full suite
5. coverage        → line/branch coverage on changed code
6. mutation tests  → do the tests actually detect broken code?
```

## Mandatory — this is the gate, not a suggestion

This skill is the **required step between "implementation done" and
everything downstream** in projects that carry a `.mido.toml` contract.
`/walkthrough` and `/lgtm` refuse to run without a `SHIP-READY` verdict for
the current revision, so skipping it blocks the whole pipeline — that is
deliberate.

- Nothing is "done" until the ladder has run on the final revision. Not the
  commit series, not the walkthrough, not the PR.
- **The verdict belongs to a revision.** Record `git rev-parse HEAD` and the
  dirty state (`git status --short`) at verification time. If any tracked file
  changed afterwards, the verdict is stale and the ladder re-runs. A stale
  PASS is not a PASS.
- **Persist the evidence.** The report below is what the next step reads; if
  you can't point at it, the next step re-runs the ladder instead of trusting
  a summary.
- `BLOCKED` / `INCOMPLETE` verdicts get stated loudly — never quietly carried
  into a walkthrough, a commit message, or a PR body.

Cheapest, most mechanical checks run first so a failure there costs seconds,
not a mutation-testing cycle.

**Guiding rule:** a failing gate is information, not an obstacle. Default
assumption: the gate is right and the code is wrong. The gate is wrong only
when the *user* says so.

Do not stop and ask permission between gates. Run the whole ladder, then
report. Stop early only per the Failure Protocol's attempt cap or an
unclear-requirement exception.

User-supplied arguments: `$ARGUMENTS`

- A path/glob → pass it to `mido --path <path>` (repeatable): the run measures
  exactly the files, folders or targets you name instead of the diff.
- `--gate <name>` → pass it to `mido --gate <name>` (repeatable): only those
  gates run. A partial selection is not a full ladder — say which gates ran
  and never present a partial run as a whole-ladder SHIP-READY.

## Phase 0 — Recon (no gate, do this first)

1. **Read `.mido.toml` at the repo root — it is the tool contract.**
   This file names the tool each gate uses and its thresholds, and it beats
   every other source, including your ecosystem instincts and the CI
   workflow:

   - **Found** → `mido` runs exactly the commands and thresholds written
     there. Announce them ("syntax gate: `cargo clippy -D warnings`, per
     `.mido.toml`") and don't second-guess a listed tool. In this repo
     `mido` implements the whole ladder: run it (see "Running this repo's
     ladder" below) instead of driving the gates by hand.
   - **Missing, or silent about a gate you need** → ask the user to add it.
     Don't coin-flip between two plausible runners when a one-line answer
     exists. Draft the block from what the stack needs (schema below), show
     it, and write the file only after the user confirms. A missing
     `.mido.toml` still runs — mido falls back to the language module's
     built-in baseline (rust ships one), with a warning — but this skill still
     asks before creating one. Never invent a `.mido.toml` silently.
   - **User unavailable / says "just go"** → proceed on best inference for
     that gate and stamp the report `tooling: inferred (<gate>)`, so the
     verdict is traceable.

2. Determine what this change actually is:

```bash
git rev-parse --show-toplevel
git status --short
git diff --stat
git diff --stat @{u}... 2>/dev/null || git diff --stat $(git merge-base HEAD origin/HEAD)
```

3. Detect the stack and cross-check. Read the manifest (`Cargo.toml`,
   `package.json`, `pyproject.toml`, `go.mod`, `Makefile`). Precedence, most
   authoritative first:

   1. `.mido.toml` — explicit project intent for these gates
   2. other project config (ESLint `max-lines` / `complexity`, `clippy.toml`,
      `ruff` `C901`, `codecov.yml`, `--cov-fail-under`)
   3. the CI workflow in `.github/workflows/` — ground truth for what the
      repo actually enforces on merge
   4. the ecosystem defaults in the gate tables below

   If `.mido.toml` disagrees with CI, the file wins — it is intent, CI may
   be stale — and report the drift in the final report so the user can
   reconcile them.

4. Missing tool (not installed, or named by `.mido.toml` but absent)?
   Do not silently skip and do not install without asking. Record the gate as
   `BLOCKED-NOT-INSTALLED` with the install command and keep going — a
   blocked gate makes the final verdict **INCOMPLETE**, never PASS. In this
   repo the fix is `nix develop`, which brings every gate tool with it.

### `.mido.toml` — the tool contract

Repo root, optional, hand-maintained. Flat sections, one per gate, named after
the gate — plus `[failure]` and optional `[targets.<name>]` sections. Every
section and key is optional; an omitted key means the gate's documented
default, and a gate with no section at all falls back to step 3's precedence
plus the `tooling: inferred` stamp.

#### Format rules

| Element | Meaning |
| --- | --- |
| `version = 1` | Required when the file exists. Only changes on a breaking format change; without it the run warns. |
| `[syntax] [size] [analysis] [tests] [coverage] [mutation]` | One flat section per gate — same names and same order as the ladder, so the file reads top-to-bottom like the run does. |
| `[targets.<name>]` | Per-target overrides: `path`, `scope`, `manifest`, and gate sections that win over the root ones. |
| `enabled = false` | Inside any gate section. The gate reports `SKIPPED` — and a skipped gate is not a pass: the run is blocked until the waiver is written down (in the report, not in the config). |
| `command` / `format` / `lint` / `typecheck` | An **argv array**, never a shell line: `["cargo", "test"]`, not `"cargo test | tail"`. A string command is a config error with the migration example in the hint. Commands are run verbatim; scoping to the target's files is the runner's job, so there is no `{paths}` placeholder. |
| `_min` / `_max` keys | Literal, self-describing limits: `mi_min = 20` fails below 20, `cognitive_max = 15` fails above 15, `changed_file_min = 80`, `total_drop_max = 0`, `kill_rate_min = 70`. **No magic values** — to switch a check off, write a permissive literal (`mi_min = 0`) that reads as exactly what it is. |
| `{ warn = X, fail = Y }` | Two-sided threshold where a warning band helps, as in `[size]` (`file_loc`, `function_loc`, `complexity`, `nesting`). |
| Unknown key or section | **Config error.** A typo like `[test]` or `min_mi` must not silently disable a gate or change a threshold. Report the offending key, stop, ask — never guess the intent. |

Tools the runner actually drives: `[size] tool` is `tokei` only, `[analysis]
tool` is `rust-code-analysis` only. Naming anything else stops that gate — an
unsupported tool is never a pass.

Defaults this file overrides, in one place: `[size]` file 300 warn / 500 fail,
function 40/60, complexity 10/15, nesting 3/4; MI ≥ `mi_min` 20; cognitive ≤
`cognitive_max` 15; changed-file coverage ≥ 80%; total coverage drop ≤ 0;
mutation kill rate ≥ 70%; timeouts syntax 1800s, tests 900s, mutation 3600s;
3 attempts per gate.

```toml
# .mido.toml — mido gate configuration
version = 1

[syntax]                         # gate 1 — formatter, linter, type checker (argv arrays)
format    = ["cargo", "fmt", "--check"]
lint      = ["cargo", "clippy", "--all-targets", "--all-features", "--", "-D", "warnings"]
typecheck = ["cargo", "check"]

[size]                           # gate 2 — size and complexity ceilings
tool         = "tokei"                     # mido drives tokei
file_loc     = { warn = 300, fail = 500 }  # non-blank, non-comment lines
function_loc = { warn = 40,  fail = 60  }
complexity   = { warn = 10,  fail = 15  }
nesting      = { warn = 3,   fail = 4   }

[analysis]                       # gate 3 — maintainability metrics
tool          = "rust-code-analysis"  # mido drives rust-code-analysis
mi_min        = 20              # maintainability index floor
cognitive_max = 15              # per function

[tests]                          # gate 4 — the full suite
command      = ["cargo", "test", "--all-features"]
timeout_secs = 900

[coverage]                       # gate 5 — coverage of changed code
command          = ["cargo", "llvm-cov", "--all-features", "--lcov", "--output-path", "lcov.info"]
changed_file_min = 80           # percent, changed files with new logic
total_drop_max   = 0            # percentage points allowed versus baseline

[mutation]                       # gate 6 — do the tests detect broken code?
command       = ["cargo", "mutants"]
scope         = "changed"       # "changed" | path | glob
timeout_secs  = 3600
kill_rate_min = 70              # percent of mutants killed

[failure]
max_attempts_per_gate = 3

# [targets.frontend]             # optional per-target overrides
# path     = "frontend"          # directory holding the crate
# scope    = ["frontend/"]       # changed-file prefixes this target owns
# manifest = "frontend/Cargo.toml"
```

Rules for this file:

- **Never edit it to make a gate pass.** Changing a threshold is the user's
  decision, stated out loud, and reported as a waiver in the final report.
- **Never create it unilaterally.** Propose the content; write it on
  confirmation.
- Read it before running any gate, and say which tools and thresholds you took
  from it. A silent config is how a gate quietly stops matching the project.
- Missing gate key ≠ permission to skip the gate. It means "default, or infer
  and stamp".

## Running this repo's ladder

This repo verifies itself: `mido` is its own ladder runner, so run it rather
than driving the gates by hand. The gate sections below describe *what `mido`
must verify* — they are not commands for you to run manually.

- **Run it from the repo root** — `cargo run --release -- <target>` (or the
  installed `mido` binary), from a `nix develop` shell so the gate tools
  (tokei, rust-code-analysis, cargo-llvm-cov, cargo-mutants) are on PATH.
- Pass `$ARGUMENTS` through in the runner's own terms — `mido` takes
  `--gate <name>` and `--path <path>` (both repeatable), a target name,
  `--lang <module>` to force a language module (rust is inferred from a root
  `Cargo.toml`), or `--all`. Don't invent flags; its `--help` is the authority
  when unsure.
- **Its exit code is the verdict**, in this skill's own vocabulary: `0`
  SHIP-READY, `1` BLOCKED, `2` INCOMPLETE.
- **Read the report it writes** — by default
  `$COMMANDCODE_SCRATCHPAD/guardrails-report.md` (`--report PATH` overrides),
  the same handoff artifact this skill requires. If the report is missing the
  verdict line, the gate table, or the revision stamp, complete it rather
  than replacing it.
- **A runner that did not run is a gate that did not run.** A missing
  interpreter, a crash, or output that cannot be read as a verdict is
  `INCOMPLETE` — never a pass, and never a silent fallback to hand-driving
  the gates. Do not re-run by hand what the runner already ran; its verdict
  stands for the revision it ran on.

## Gate 1 — Syntax guards

Formatter, linter, type checker, compiler. Everything the ecosystem
enforces mechanically.

| Stack | Commands |
| --- | --- |
| Rust | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo check` |
| TS/JS | `npx tsc --noEmit` (or `npx tsgo --noEmit`), `npx eslint .` / `npx biome check .`, `npx prettier --check .` |
| Python | `ruff format --check .`, `ruff check .`, `mypy .` / `pyright` |
| Go | `gofmt -l .`, `go vet ./...`, `staticcheck ./...` |

Rules:

- `.mido.toml` supplies these as `[syntax] format`, `[syntax] lint`,
  `[syntax] typecheck` — argv arrays, run in that order. A single
  `[syntax] command` runs first when declared.
- **Warnings are failures.** Add `-D warnings` / `--max-warnings=0` if the
  project's CI does. A new warning introduced by this change is a fail even
  if the tool exits 0.
- **Formatting is the one auto-fixable failure.** If `--check` mode fails
  and the only diff is formatting, run the formatter in write mode, confirm
  the diff is formatting-only (`git diff --stat`, no logic lines), and pass
  the gate. Never hand-edit formatting.
- **Never add an allow.** No `#[allow(...)]`, `eslint-disable`, `# noqa`,
  `# type: ignore`, `@ts-expect-error`, `nolint`. If a lint is genuinely
  wrong for this code, that's a config change for the user to approve.
- Type errors in tests are still type errors. Fix them.

## Gate 2 — Module size verification

Measure the size of what this change produced. Growing a file is a design
decision the change now owns.

Default thresholds (per changed file/function). `.mido.toml` `[size]`
keys — `file_loc`, `function_loc`, `complexity`, `nesting`, each
`{ warn, fail }` — then existing linter limits, override them:

| Metric | Warn | Fail |
| --- | --- | --- |
| File LOC (non-blank, non-comment) | 300 | 500 |
| Function/method length | 40 | 60 |
| Cyclomatic/cognitive complexity | 10 | 15 |
| Nesting depth | 3 | 4 |

How to measure:

- **Rust:** the runner drives `tokei` for LOC and `rust-code-analysis` for
  the function metrics; by hand,
  `cargo clippy -- -W clippy::too_many_lines -W clippy::cognitive_complexity`
  is the quick probe. mido accepts only `tool = "tokei"` here.
- **TS/JS:** ESLint `max-lines`, `max-lines-per-function`, `complexity`
  if configured; otherwise `npx scc <changed paths>` or a small script for
  LOC + a complexity pass from `eslint --rule '{"complexity": ["error", 15]}'`.
- **Python:** `radon cc -s -n C <changed paths>`, `radon mi` for
  maintainability index.
- **Go:** `gocyclo -over 15 <files>`, `funlen` via golangci-lint.

Report the numbers even when passing — the trend matters, and the user sees
whether a file is drifting toward the limit.

Division of labor with gate 3: this gate enforces hard ceilings on size and
raw complexity; gate 3 judges maintainability *quality* (MI, cognitive) on
the same code. The complexity overlap is deliberate — a ceiling here, a
trend view there. Pick one tool and use it for both rather than running two
complexity counters with different definitions.

**Anti-gaming rule.** When a file exceeds the limit, the fix is *design*,
not relocation. Moving 400 lines of code into `helpers.rs` without changing
responsibilities does not pass this gate — it makes the problem harder to
find. Split along a real seam: extract a cohesive module with its own name,
data, and interface, and leave the original thinner. If no honest seam
exists, say so and report it as a design problem to resolve with the user
rather than fake-passing the gate.

## Gate 3 — Static analysis (maintainability metrics)

The size gate asked *how big*; this asks *how hard it is to hold in your head* —
maintainability index, cognitive complexity. Same static-structure axis as
size, so it runs here, before anything dynamic.

Default tool: [rust-code-analysis](https://github.com/mozilla/rust-code-analysis)
(Mozilla, tree-sitter based) — the only `[analysis] tool` the runner drives.
Languages it actually parses: **Rust, C/C++, Java, JavaScript, TypeScript,
Python** — nothing else, so other stacks need the fallbacks below.

```bash
rust-code-analysis-cli -m -O json -o .guardrails-analysis -p <changed paths>
# inspect one file by hand (pretty JSON to stdout):
rust-code-analysis-cli -m --pr -O json -p src/foo.rs
```

`-I` / `-X` add include/exclude globs, `-j` sets parallelism, `-f error` finds
syntax-error nodes. Metrics come out as a nested `spaces` tree (JSON, TOML, YAML
or CBOR) where every node carries the metric families: `ABC`, cyclomatic `CC`,
`COGNITIVE`, `HALSTEAD`, `MI`, `NOM`, `NARGS`, `NEXITS`, `WMC`, and the LoC
family `SLOC`/`PLOC`/`LLOC`/`CLOC`/`BLANK`.

**The CLI has no threshold flag** — verified against the current CLI source, which
has `-m`, `-p`, `-O`, `-o`, `-I`, `-X`, `-j`, `--pr`, `-f`, `--ls`/`--le`, but no
limits option. So this gate enforces limits *after* parsing that JSON: dump one
changed file pretty first and read the real key names, since they vary by version
and language. Do not invent flag names for this tool.

Fallbacks when the stack isn't covered or the tool is absent: `lizard`
(multi-language), `radon cc` / `radon mi` (Python), `gocyclo` (Go), `jscpd` or
PMD CPD (duplication, language-agnostic).

Judged by default (`.mido.toml` `[analysis]` overrides):

| Metric | Default | Notes |
| --- | --- | --- |
| Maintainability index (MI) | fail below `mi_min` (20) | 0–100, higher better; judge the worst changed unit |
| Cognitive complexity | fail above `cognitive_max` (15) | per function; same ceiling as the size gate |

In a mido contract, `[analysis]` takes only `tool`, `mi_min` and
`cognitive_max` (`enabled` aside); a `command`, `halstead_effort_max` or
`duplication_command` key is a config error, and Halstead/duplication are
never gated — report them if the tool prints them, don't invent a threshold.

Rules:

- Scope to the changed files, and name the worst offender with numbers:
  `src/parser.rs:120 parse_config — MI 14, cognitive 22`. "PASS" with no numbers
  tells the next reader nothing.
- Repo-wide metric debt is not this change's problem. Don't hard-fail on
  pre-existing numbers — report them in the pre-existing section.
- **Anti-gaming:** don't raise MI by padding comments, don't split files purely to
  move a metric, don't rename identifiers to shift Halstead numbers. A bad number
  means the structure is bad; the fix is the size gate's fix — extract along a
  real seam.
- Metrics are directional, not targets — the same rule as coverage. Chasing a
  score produces worse code than ignoring it.
- List files the tool couldn't parse (unsupported language, generated code)
  explicitly. An unanalyzed changed file is `INCOMPLETE`, not a pass.

## Gate 4 — Tests

Run the project's full suite, exactly as CI does:

```bash
cargo test --all-features        # Rust
npx vitest run / npx jest        # TS/JS
pytest -q                        # Python
go test ./...                    # Go
```

`.mido.toml` `[tests]` declares `command` (argv) and `timeout_secs`.

Rules:

- Full suite, not just the tests near the diff. The whole point is finding
  what this change broke somewhere else.
- A flaky failure is not a pass. Re-run once; if it fails again, or it
  passed only on retry, report it as `FAIL (flaky)` and fix or quarantine
  with the user's explicit approval.
- **Never** delete, rename away, `.skip`, `#[ignore]`, `@pytest.mark.skip`,
  `t.Skip`, or loosen an assertion to get green. See Failure Protocol.
- New behavior with no test is a fail at this gate even if the suite is
  green — the gate covers *this change*, not just the repo.

## Gate 5 — Coverage

Measure coverage, then judge the changed code against it.

| Stack | Command |
| --- | --- |
| Rust | `cargo llvm-cov --all-features --lcov --output-path lcov.info` (needs `cargo-llvm-cov`) |
| TS/JS | `npx vitest run --coverage` / `npx jest --coverage` |
| Python | `pytest --cov=<pkg> --cov-report=term-missing` |
| Go | `go test -coverprofile=cover.out ./... && go tool cover -func=cover.out` |

Reporting:

- Total line/branch coverage **and** per-changed-file coverage. The per-file
  number is the useful one; a big repo average hides an untested new module.
- Default bar (`.mido.toml` `[coverage]` overrides `changed_file_min` and
  `total_drop_max`): changed files containing new logic ≥ 80% line coverage,
  and no drop in total coverage beyond 0 points versus the pre-change baseline
  (measure with `git stash` / a clean checkout if needed, and say how you
  measured it; `mido --baseline-lcov` takes the base revision's lcov file).
- Other project config (codecov.yml, `--cov-fail-under`, vitest thresholds)
  also overrides the default — but ranks below `.mido.toml`.

Coverage is a *map of untested paths*, not a target. Never add a test whose
only purpose is to touch lines — a test with no meaningful assertion inflates
this gate and fails gate 6. If a line is genuinely untestable (platform
branch, unreachable defensive arm), say which line and why in the report
instead of hiding it under a blanket exclude.

## Gate 6 — Mutation testing

The honesty check: do the tests *detect* broken code? Coverage says tests ran
the line; mutation testing says the tests would notice if the line were wrong.

| Stack | Command |
| --- | --- |
| Rust | `cargo mutants --in-place` (scoped: `--file src/changed.rs`) |
| TS/JS | `npx stryker run` with `mutate` scoped to changed files |
| Python | `mutmut run --paths-to-mutate src/changed_module.py` |
| Go | `gremlins unleash ./changed/pkg` |

`.mido.toml` `[mutation]` declares `command`, `scope` (`"changed"` | path |
glob), `timeout_secs` and `kill_rate_min`; the changed scope writes the
working-tree patch and scopes mutants to it, explicit paths scope the named
files.

This gate is slow — minutes to hours — so:

- **Scope to the changed modules**, never the whole repo, unless the user asks.
- Run it as a background task (`run_in_background`) or under `monitor_command`
  and keep working/reporting; do not hold the session hostage. Enforce the
  `[mutation] timeout_secs` budget and report `TIMEOUT` with the mutants
  analyzed so far rather than waiting forever.
- Start from the changed-files list from Phase 0 so scoping is a fact, not a guess.

Bar: **`kill_rate_min` (`.mido.toml` `[mutation]`, default 70%) of
mutants killed** on the changed modules, and **zero unexplained survivors**.
Each surviving mutant is one of:

1. **Missing/weak assertion** → the test ran the code but didn't check the
   outcome. Fix the test to assert the behavior the mutant broke.
2. **Untested branch** → add the test (as a real behavior test, per the
   `tdd` skill's standards).
3. **Equivalent mutant** → the mutation genuinely cannot change behavior
   (e.g. `x + 0` → `x - 0`, or a mutation in dead/log-only code). Allowed,
   but requires a one-line written justification and a pointer to the line.
   "Hard to test" is not equivalent.

Never mark a survivor equivalent in bulk, and never exclude a module to lift
the score.

## Failure Protocol

Applies to every gate. This is the skill's core behavior: a failure is a
signal about implementation versus intent, and the response is to reason
toward the intent — not to silence the signal.

For each failure, work this loop explicitly (write it down in the report;
the reasoning is a deliverable):

1. **Desired behavior/state.** One sentence, traceable to a source: the
   failing test's name, the spec/issue, `WALKTHROUGH.md`, the doc comment, or
   the user's request. Not "make the linter happy" — what should the system
   *do*?
2. **Current behavior/state.** One sentence, with exact evidence: the failing
   test name and assertion, the mutant diff and its line, the measured LOC
   against the limit, the clippy lint and code position.
3. **Gap.** Name which kind of gap it is:
   - *implementation bug* — behavior is wrong; the code must change
   - *missing behavior* — the code never handled this case
   - *test gap* — implementation is right, the test doesn't check the right
     thing (common mutation survivor)
   - *design smell* — too big / too complex; the structure must change
     (module size gate)
   - *guardrail config wrong* — the threshold/rule doesn't fit this project;
     a decision for the user, never a silent edit
4. **Smallest step.** One hypothesis, the minimum change that moves toward
   the desired state. Not a rewrite, not a drive-by refactor of neighboring
   code.
5. **Apply and re-enter.** Make the change, then re-run from the re-entry
   point below.
6. **Cap: `[failure] max_attempts_per_gate` (default 3) distinct hypotheses
   per gate.** Same failure three times means the model of the problem is
   wrong, not the fix. Stop, and escalate.

**Escalate** (stop, report, ask) when:

- `[failure] max_attempts_per_gate` attempts on one gate are exhausted.
- The desired behavior is genuinely ambiguous — the tests contradict the
  spec, the spec is silent, or two sources disagree. Do not pick a
  behavior and hope. This is the one place guardrails may halt mid-ladder.
- A fix would require changing a guardrail's config, thresholds, or scope.
- You doubt which tool a gate should use and `.mido.toml` doesn't say.
  Ask; that's a one-line answer, not a research project.
- The fix belongs outside this change's boundary (a pre-existing failure,
  a missing dependency, an unrelated broken test in another module). Report
  it as pre-existing with evidence (`git stash` → still fails → not yours)
  and leave it alone; don't fix the world inside this change.

**Never, under any pressure:**

- Weaken, disable, delete, or skip a check, test, or assertion to pass.
- Change a test's expectation to match the implementation's actual output
  when the expectation encoded the desired behavior.
- Make a threshold pass by moving code, excluding files, or bumping config.
- Commit or report PASS on a gate that was skipped, blocked, or timed out.

**Exception:** when the test itself is wrong — it asserts behavior the user
never wanted — fixing the test is correct. But it must be stated explicitly
in the report: "test asserted X, user/spec says Y, changed the test to Y,"
never a silent edit. When in doubt between "test is wrong" and "code is
wrong", the code is wrong until the user says otherwise.

## Re-entry rules

Any semantic code change invalidates downstream gates:

| Change made at | Re-run from |
| --- | --- |
| Gate 1, formatting only (formatter write-mode) | same gate — continue forward |
| Gate 1, any logic/type change | Gate 1 |
| Gates 2–6, any source change | Gate 1 (cheap gates first; they'll be fast) |
| Gates 4–6, test-only change | Gate 4 |
| Config/threshold change (user-approved only) | the gate it configures, then forward |

A gate that passed before a code change is not evidence any more. Do not
carry a stale PASS forward.

## Final report

One compact block, gate by gate. No essays.

| Gate | Status | Evidence |
| --- | --- | --- |
| 1 Syntax | PASS | `cargo clippy -D warnings` clean; fmt applied to 2 files |
| 2 Module size | PASS | max changed file 214 LOC; max fn 38 lines |
| 3 Analysis | PASS | worst unit `parser.rs:120 parse_config` MI 41, cognitive 11 |
| 4 Tests | PASS | 412 passed, 0 failed (`cargo test --all-features`) |
| 5 Coverage | PASS | changed files 91% lines; total 87.4% (was 87.1%) |
| 6 Mutation | FAIL→PASS | 74% killed after adding 3 assertions; 2 equivalent mutants justified |

Then:

- **Verdict:** `SHIP-READY` only when gates 1–4 pass (syntax, size, analysis,
  tests) and 5–6 either pass or were explicitly waived by the user (name the
  waiver). Otherwise `BLOCKED` (a gate failed and the cap was hit / behavior is
  ambiguous) or `INCOMPLETE` (a tool was missing, a changed file went
  unanalyzed, or a gate timed out).
- **Fix log:** each failure, the desired-state/current-state/gap reasoning,
  what changed, which gate re-run — one line per iteration.
- **Surviving mutants / untested lines,** with the justification for each.
- **Pre-existing issues found but not touched.**
- **The report is required, not optional.** It belongs at
  `$COMMANDCODE_SCRATCHPAD/guardrails-report.md` and you give the path — it is
  the handoff artifact the next step reads. The ladder runner writes
  it (`mido --report PATH` overrides the location): check the file, complete
  it if incomplete, don't duplicate it. It must contain three things: the
  verdict line, the per-gate table, and the **revision stamp**:
  `git rev-parse HEAD` plus `git status --short | git hash-object --stdin`.
  Only write a report into the repo itself if the user asks.

## Hard rules

- Gates run in order, all of them, every time. No skipping, no reordering.
- **Never hand off to `/walkthrough` or `/lgtm` without a `SHIP-READY` verdict
  for the exact revision being handed off, with the report on disk as
  evidence.** If that verdict doesn't exist, run the ladder — don't narrate
  around it.
- `.mido.toml` decides which tool each gate runs. In this repo, `mido` runs
  the whole ladder: invoke it from the repo root (`cargo run --`), don't
  reimplement the ladder by hand around it. Missing file → ask, don't guess.
  Never create or edit it without the user's say-so, and never touch a
  threshold to turn RED into GREEN.
- A gate that did not run is not a gate that passed.
- Never weaken a check to make it pass — that is lying to the next reader.
- Never fix an unrelated pre-existing failure inside this change.
- Formatting is the only auto-fix; everything else is a decision.
- Every failure gets the desired-state → current-state → gap → smallest-step
  reasoning written down, not performed silently.
- Three tries per gate, then escalate. Thrashing is worse than escalating.
- Ambiguous desired behavior stops the ladder and goes to the user.
