# flash-verify

`flash-verify` is a Rust std-only verification engine POC. The first target is a
fast iterative verifier for the sibling `../eli` checkout.

The source of truth is a usage-driven end-to-end benchmark/eval suite. A task is
a user demand plus a driver, evidence capture, hard gates, and a scoreable
rubric. Commands are only how a task produces evidence; command success alone is
not the product verdict.

The initial architecture is intentionally small:

```text
suite -> task -> driver -> bounded evidence -> grader -> report
```

## Current POC

- The built-in `eli` benchmark has `quick`, `cli-smoke`, `smoke`, `rust`,
  `sidecar-feishu`, `gateway-core`, `hard-tail`, and `full` suites.
- Each task runs list-form argv with a timeout.
- stdout/stderr are captured with bounded head/tail snippets, while assertions
  scan the full stream for declared needles.
- The grader records hard gates, score, demand, command, duration, and snippets.
- The CLI exits `0` only when every task in every iteration passes.
- `--task ID` expands declared dependencies first, then runs the requested task.
- Stop-fast runs still score against planned task max points and report
  planned/executed task counts.

`hard-tail` in flash-verify is the offline contract check for the benchmark
fixture and scorer. Eli's actual hard-tail runner remains the source of live
model quality evidence: `../eli/scripts/run_model_comparison_suite.py` calls
`eli run`, sends real model API requests, and scores the hard-tail cases.

Example:

```bash
cargo run -- bench eli --repo ../eli --suite quick
```

Machine-readable output:

```bash
cargo run -- bench eli --repo ../eli --suite quick --json
```

Artifact report:

```bash
cargo run -- bench eli --repo ../eli --suite quick --report /tmp/flash-verify
```

Run tests:

```bash
cargo test
```

## Architecture Direction

The project should keep three boundaries clear:

1. Benchmark model: define usage demands, task ids, thresholds, and rubrics.
2. Execution engine: run drivers with strict timeout and bounded output.
3. Evidence report: produce machine-readable verdicts with enough context to
   explain every failure and rerun the exact task.

Near-term extensions should stay data-driven: external suite specs, cache keys,
adversarial task variants, and report sinks can layer on top of the current Eli
suite once the runner is proven.
