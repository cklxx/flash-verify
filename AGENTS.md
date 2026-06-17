# flash-verify - Agent Contract

Assisting ckl. Project-specific rules only. `AGENTS.md` is the single source of
truth; `CLAUDE.md` should be a symlink to this file so agent rules cannot drift.

---

## First Principle

Evidence beats inference. Source reading, architecture guesses, and callgraph
inspection are hypotheses until a runnable check, captured output, or structured
report proves the claim.

- Keep the scope narrow and explicit before editing.
- Label uncertain conclusions as hypotheses.
- Prefer the cheapest decisive verification over broad sweeps.
- Isolate confounders: one changed variable per experiment unless the report
  explicitly calls out why attribution is impossible.
- For any non-trivial change, leave behind a reproducible command.

---

## Project Shape

`flash-verify` is a Rust std-only verification engine.

Single source of truth: a usage-driven end-to-end benchmark/eval suite. A task
is a user demand plus a driver, captured evidence, hard gates, and a scoreable
rubric. Commands are only drivers that produce evidence; a green command is not
the truth by itself.

The core flow is:

```
suite -> task -> driver -> bounded evidence -> grader -> report -> exit code
```

Current POC boundaries:

- `src/eli.rs` owns the first built-in benchmark suites for `../eli`.
- `src/runner.rs` owns command execution, timeout handling, iterations, and
  usage-task grading/report assembly.
- `src/capture.rs` owns bounded stdout/stderr capture.
- `src/cli.rs` is the CLI front door.
- `examples/` contains runnable usage notes.
- Unit tests live next to the Rust modules they cover.

Architectural invariant: execution side effects stay inside the runner, reports
are structured JSON-compatible data, and default verification avoids live IM/API
side effects.

---

## Execution Phases

Use this for non-trivial tasks:

| Phase | Exit condition |
| --- | --- |
| Explore | You can name the files you will touch. |
| Plan | Architectural or >3-file changes have a written approach. |
| Implement | The diff follows the agreed boundary and existing style. |
| Verify | Relevant commands pass, or the blocker is stated with exact output. |
| Reflect | Any repeated miss creates a concrete rule or TODO. |

Small isolated edits may go straight to Implement + Verify.

---

## Editing

- Preserve unrelated work by default.
- Do not widen Git, shell, or filesystem side effects beyond the current repo
  unless explicitly asked.
- Git identity changes are local (`git config --local`) unless the user
  explicitly asks for global config.
- Keep suites data-driven. Do not bake one-off task checks into the runner when
  a task field can express them.
- Use list-form commands as task drivers. Shell execution is out of scope for
  the POC.
- Every executed command must have a timeout.
- Reports must include enough evidence to understand the pass/fail verdict:
  demand, command, exit code, duration, stdout/stderr snippets, hard gates, and
  score.
- Assertions are evaluated from full-stream observations collected during
  capture. The report may store bounded head/tail evidence, but a forbidden
  needle in truncated output must still fail the task.
- Single-task selection must include declared dependencies before the selected
  task so `--task` is deterministic from a clean checkout.
- Offline harness checks do not prove Eli model quality. Final Eli quality
  claims require the existing hard-tail runner to send real model API requests
  through `eli run` against sufficiently hard rubric cases.
- Avoid dependencies until a real need appears. The POC should stay Rust
  std-only.

---

## Verification

Canonical local checks:

```bash
cargo fmt --all -- --check
cargo test
cargo run -- bench eli --repo ../eli --suite quick
cargo run -- bench eli --repo ../eli --suite quick --json
```

Expected CLI behavior:

- Exit `0` when every task passes.
- Exit `1` when any task fails or times out.
- Exit `2` when CLI/task selection is invalid.
- Exit `3` when the harness/report writer fails.
- `--json` writes only machine-readable JSON to stdout.
- `--events` writes NDJSON progress events to stderr.
- `--report DIR` creates a per-run artifact directory and must not overwrite a
  prior run.

---

## Build And Run

Build with Cargo:

```bash
cargo run -- bench eli --repo ../eli --suite quick
```

Installable packaging may be added later, but the repository must remain runnable
from checkout during early architecture work.

---

## Documentation

Start with:

- `README.md` for the project overview and current POC architecture.
- `docs/architecture.md` for module boundaries and next-step design notes.
