# Eli Smoke Verification

Run the default offline benchmark against the sibling Eli checkout:

```bash
cargo run -- bench eli --repo ../eli --suite quick
```

The source of truth is a usage task: demand, driver, evidence, hard gates, and
score. Commands are drivers, not the verdict.

Useful suites:

- `quick`: build `target/debug/eli`, then verify local CLI help and usage errors.
- `cli-smoke`: `quick` plus status and missing-message validation.
- `smoke`: `cli-smoke`, `cargo check --workspace`, and `cargo test -p eli --lib`.
- `rust`: `cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`,
  then `cargo test --workspace`.
- `sidecar-feishu`: offline Feishu/Lark sidecar plugin contracts.
- `gateway-core`: offline gateway, webhook, sidecar bridge, and schema contracts.
- `hard-tail`: offline model-benchmark fixture/rubric contract checks.
- `full`: `cli-smoke + rust + sidecar-feishu + gateway-core + hard-tail`.

Useful flags:

- `--task ID`: run one stable task id.
- `--keep-going`: collect later failures after the first failed task.
- `--repeat N`: check iterative stability.
- `--json`: emit only JSON to stdout.
- `--events`: emit NDJSON progress events to stderr.
- `--report DIR`: write stdout/stderr/grade artifacts under a new run directory.
