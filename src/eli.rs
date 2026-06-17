use std::path::Path;
use std::time::Duration;

use crate::cli::{EliConfig, EliSuite};
use crate::runner::{Check, CheckKind, Suite, Task};

pub fn eli_suite(config: &EliConfig) -> Suite {
    Suite {
        id: format!("eli/{}", suite_name(config.suite)),
        repo: config.repo.clone(),
        threshold: 100,
        tasks: tasks(config),
    }
}

fn tasks(config: &EliConfig) -> Vec<Task> {
    match config.suite {
        EliSuite::Quick => quick_tasks(&config.repo),
        EliSuite::CliSmoke => cli_smoke_tasks(&config.repo),
        EliSuite::Smoke => smoke_tasks(&config.repo),
        EliSuite::Rust => rust_tasks(&config.repo),
        EliSuite::Full => full_tasks(&config.repo),
        EliSuite::SidecarFeishu => sidecar_feishu_tasks(&config.repo),
        EliSuite::GatewayCore => gateway_core_tasks(&config.repo),
        EliSuite::HardTail => hard_tail_tasks(&config.repo),
    }
}

fn quick_tasks(repo: &Path) -> Vec<Task> {
    vec![
        build_eli_bin(repo),
        cli_help(repo),
        cli_invalid_subcommand(repo),
    ]
}

fn cli_smoke_tasks(repo: &Path) -> Vec<Task> {
    vec![
        build_eli_bin(repo),
        cli_help(repo),
        cli_status(repo),
        cli_invalid_subcommand(repo),
        cli_run_requires_message(repo),
    ]
}

fn smoke_tasks(repo: &Path) -> Vec<Task> {
    let mut tasks = cli_smoke_tasks(repo);
    tasks.extend([cargo_check(repo), eli_lib_tests(repo)]);
    tasks
}

fn rust_tasks(repo: &Path) -> Vec<Task> {
    vec![
        cargo_fmt(repo),
        cargo_clippy(repo),
        cargo_workspace_tests(repo),
    ]
}

fn full_tasks(repo: &Path) -> Vec<Task> {
    let mut tasks = cli_smoke_tasks(repo);
    tasks.extend(rust_tasks(repo));
    tasks.extend(sidecar_feishu_tasks(repo));
    tasks.extend(gateway_core_tasks(repo));
    tasks.extend(hard_tail_tasks(repo));
    tasks
}

fn sidecar_feishu_tasks(repo: &Path) -> Vec<Task> {
    vec![
        task(
            "feishu-sidecar-contract",
            "Feishu sidecar plugin handles dedup, chunking, commands, and typing behavior offline",
            repo.join("sidecar"),
            &["bun", "test", "test/feishu-cli-plugin.test.ts"],
            180,
            vec![exit(0), output_forbids("FAIL"), output_forbids("panic")],
        ),
        task(
            "lark-plugin-contract",
            "Lark/OpenClaw sidecar registration remains loadable without live IM traffic",
            repo.join("sidecar"),
            &["bun", "test", "test/lark-plugin.test.ts"],
            180,
            vec![exit(0), output_forbids("FAIL"), output_forbids("panic")],
        ),
    ]
}

fn gateway_core_tasks(repo: &Path) -> Vec<Task> {
    vec![
        task(
            "gateway-core-rust",
            "Gateway command tests preserve lock, shutdown, and channel startup semantics",
            repo,
            &["cargo", "test", "-p", "eli", "gateway"],
            180,
            vec![
                exit(0),
                stdout_has("test result: ok"),
                output_forbids("panic"),
            ],
        ),
        task(
            "webhook-rust-core",
            "Webhook channel payload tests preserve outbound routing and media contract shape",
            repo,
            &["cargo", "test", "-p", "eli", "webhook"],
            180,
            vec![
                exit(0),
                stdout_has("test result: ok"),
                output_forbids("panic"),
            ],
        ),
        task(
            "sidecar-contract-rust",
            "Rust sidecar contract fixtures remain schema-compatible with the TypeScript sidecar",
            repo,
            &["cargo", "test", "-p", "eli", "sidecar_contract"],
            180,
            vec![
                exit(0),
                stdout_has("test result: ok"),
                output_forbids("panic"),
            ],
        ),
        task(
            "sidecar-bridge-contract",
            "Sidecar bridge preserves inbound, outbound, envelope, and schema contracts offline",
            repo.join("sidecar"),
            &[
                "bun",
                "test",
                "test/bridge.test.ts",
                "test/envelope.test.ts",
                "test/contract.test.ts",
            ],
            180,
            vec![exit(0), output_forbids("FAIL"), output_forbids("panic")],
        ),
    ]
}

fn hard_tail_tasks(repo: &Path) -> Vec<Task> {
    vec![task(
        "hard-tail-benchmark-contract",
        "Hard-tail model benchmark cases and rubric stay diverse, scored, and machine-gradable",
        repo,
        &[
            "python3",
            "-m",
            "pytest",
            "tests/test_model_comparison_suite.py",
            "-q",
        ],
        180,
        vec![exit(0), output_forbids("FAILED"), output_forbids("panic")],
    )]
}

fn build_eli_bin(repo: &Path) -> Task {
    task(
        "build-eli-bin",
        "User needs a reusable local Eli binary before interactive CLI tasks run",
        repo,
        &["cargo", "build", "-p", "eli", "--bin", "eli"],
        180,
        vec![exit(0), output_forbids("error:"), output_forbids("panic")],
    )
}

fn cli_help(repo: &Path) -> Task {
    depends_on_build(task(
        "cli-help",
        "User asks Eli for CLI help and sees the command surface",
        repo,
        &["./target/debug/eli", "--help"],
        10,
        vec![
            exit(0),
            stdout_has("Usage: eli <COMMAND>"),
            stdout_has("Commands:"),
            output_forbids("panic"),
        ],
    ))
}

fn cli_status(repo: &Path) -> Task {
    depends_on_build(task(
        "cli-status",
        "User asks Eli for local auth/config status without model execution",
        repo,
        &["./target/debug/eli", "status"],
        10,
        vec![
            exit(0),
            stdout_has("Eli configuration status"),
            stdout_has("Active profile"),
            output_forbids("panic"),
        ],
    ))
}

fn cli_invalid_subcommand(repo: &Path) -> Task {
    depends_on_build(task(
        "cli-invalid-subcommand",
        "User mistypes a command and receives a local CLI error",
        repo,
        &["./target/debug/eli", "definitely-not-a-command"],
        10,
        vec![
            Check::hard(
                "invalid command exits with usage error",
                4,
                CheckKind::ExitCode(2),
            ),
            stderr_has("unrecognized subcommand"),
            stderr_has("Usage: eli <COMMAND>"),
            output_forbids("panic"),
        ],
    ))
}

fn cli_run_requires_message(repo: &Path) -> Task {
    depends_on_build(task(
        "cli-run-requires-message",
        "User invokes eli run without a message and gets a local validation error",
        repo,
        &["./target/debug/eli", "run"],
        10,
        vec![
            Check::hard(
                "missing message exits with usage error",
                4,
                CheckKind::ExitCode(2),
            ),
            stderr_has("<MESSAGE>"),
            stderr_has("Usage: eli run <MESSAGE>"),
            output_forbids("panic"),
        ],
    ))
}

fn cargo_check(repo: &Path) -> Task {
    task(
        "cargo-check",
        "Eli workspace typechecks after the current change",
        repo,
        &["cargo", "check", "--workspace"],
        180,
        vec![exit(0), output_forbids("error:"), output_forbids("panic")],
    )
}

fn eli_lib_tests(repo: &Path) -> Task {
    task(
        "eli-lib-tests",
        "Eli core library behavior remains internally consistent",
        repo,
        &["cargo", "test", "-p", "eli", "--lib"],
        180,
        vec![
            exit(0),
            stdout_has("test result: ok"),
            output_forbids("panic"),
        ],
    )
}

fn cargo_fmt(repo: &Path) -> Task {
    task(
        "fmt",
        "Rust formatting stays canonical",
        repo,
        &["cargo", "fmt", "--all", "--", "--check"],
        60,
        vec![exit(0)],
    )
}

fn cargo_clippy(repo: &Path) -> Task {
    task(
        "clippy",
        "Rust lints stay warning-free",
        repo,
        &["cargo", "clippy", "--workspace", "--", "-D", "warnings"],
        300,
        vec![
            exit(0),
            output_forbids("warning:"),
            output_forbids("error:"),
        ],
    )
}

fn cargo_workspace_tests(repo: &Path) -> Task {
    task(
        "workspace-tests",
        "Entire Eli Rust workspace test suite passes",
        repo,
        &["cargo", "test", "--workspace"],
        300,
        vec![
            exit(0),
            stdout_has("test result: ok"),
            output_forbids("panic"),
        ],
    )
}

fn task(
    id: &str,
    demand: &str,
    cwd: impl AsRef<Path>,
    argv: &[&str],
    timeout_secs: u64,
    checks: Vec<Check>,
) -> Task {
    Task {
        id: id.into(),
        demand: demand.into(),
        cwd: cwd.as_ref().to_path_buf(),
        argv: argv.iter().map(|arg| (*arg).into()).collect(),
        timeout: Duration::from_secs(timeout_secs),
        threshold: 100,
        dependencies: Vec::new(),
        checks,
    }
}

fn depends_on_build(mut task: Task) -> Task {
    task.dependencies.push("build-eli-bin".into());
    task
}

fn exit(code: i32) -> Check {
    Check::hard(
        &format!("exit code is {code}"),
        4,
        CheckKind::ExitCode(code),
    )
}

fn stdout_has(needle: &str) -> Check {
    Check::soft(
        &format!("stdout contains {needle:?}"),
        2,
        CheckKind::StdoutContains(needle.into()),
    )
}

fn stderr_has(needle: &str) -> Check {
    Check::soft(
        &format!("stderr contains {needle:?}"),
        2,
        CheckKind::StderrContains(needle.into()),
    )
}

fn output_forbids(needle: &str) -> Check {
    Check::hard(
        &format!("output forbids {needle:?}"),
        1,
        CheckKind::AnyOutputForbid(needle.into()),
    )
}

fn suite_name(suite: EliSuite) -> &'static str {
    match suite {
        EliSuite::Quick => "quick",
        EliSuite::CliSmoke => "cli-smoke",
        EliSuite::Smoke => "smoke",
        EliSuite::Rust => "rust",
        EliSuite::Full => "full",
        EliSuite::SidecarFeishu => "sidecar-feishu",
        EliSuite::GatewayCore => "gateway-core",
        EliSuite::HardTail => "hard-tail",
    }
}
