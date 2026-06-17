# flash-verify Architecture

## Goal

Build a small verification engine that turns usage-driven benchmark/eval suites
into reproducible evidence. The engine should make it cheap to answer: what user
demand was checked, what ran, what evidence was captured, which hard gate or
rubric failed, and what proves the verdict?

The single source of truth is the task model:

```text
task = demand + driver + evidence + hard gates + scoreable rubric
```

Commands are implementation detail. They are valid only because they drive a
task and produce evidence for the grader.

## POC Boundary

The POC is a Rust std-only implementation with one built-in control path:

```text
select eli suite
  -> select stable task ids
  -> run list-form task drivers with timeouts
  -> capture bounded stdout/stderr evidence
  -> grade hard gates and score
  -> emit human, JSON, and optional artifact report
  -> return process status
```

## Modules

- `cli`: argument parsing for the built-in benchmark runner.
- `eli`: `../eli` suite and task definitions.
- `runner`: command execution, timeout handling, iterations, grading, and report
  model.
- `capture`: bounded head/tail output capture.
- `json`: minimal JSON string rendering.

## Design Rules

- Suites are declarative Rust data for the POC. JSON specs come after the
  runner proves useful.
- Command execution is explicit and non-shell by default.
- Every command has a timeout.
- Assertion needles are scanned while streaming, so bounded evidence cannot hide
  failures in truncated middle output.
- Single-task selection expands declared dependencies before the selected task.
- Reports carry demand, evidence, gates, score, and rerun command, not just
  booleans.
- Stop-fast iterations keep planned task counts and planned max score, so a
  failed partial run is not scored as if skipped tasks did not exist.
- Default suites avoid live IM/API side effects. Live and model-backed evals
  need explicit suites.
- The engine returns a non-zero process code when any task fails.

## Eli Suite Map

- `quick`: build the Eli binary, verify CLI help, and verify local usage errors.
- `cli-smoke`: `quick` plus status and missing-message validation.
- `smoke`: `cli-smoke` plus workspace typecheck and Eli library tests.
- `rust`: formatting, clippy, and workspace Rust tests.
- `sidecar-feishu`: offline Feishu/Lark sidecar plugin contract tests.
- `gateway-core`: offline Rust gateway/webhook/contract tests plus TypeScript
  sidecar bridge contracts.
- `hard-tail`: offline model-benchmark fixture and rubric contract tests.
- `full`: `cli-smoke + rust + sidecar-feishu + gateway-core + hard-tail`.

Live provider calls stay in Eli's existing
`scripts/run_model_comparison_suite.py` path, which calls `eli run` and scores
the hard-tail cases. The flash-verify `hard-tail` suite proves that fixture and
scorer contract before those live runs.

## Open Architecture Questions

- Spec format: strict JSON is likely the first external format, but v1 avoids a
  hand-rolled parser until the runner semantics are proven.
- Assertion model: add regex, JSONPath, file existence, numeric thresholds, and
  custom plugins as separate typed assertions?
- Execution model: keep local subprocess only, or introduce remote/container
  executors behind an executor interface?
- Report sinks: stdout JSON is enough for POC; CI may need JUnit, Markdown, or
  artifact directories.
- Isolation: future command specs may need cwd/env controls, resource limits, and
  cleanup hooks.

## Next POC Slice

Add a strict external suite front-end that compiles to the same `Task` model as
the built-in Eli suites. That is the first useful boundary test for whether the
task/report split is clean enough.
