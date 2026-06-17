mod capture;
mod cli;
mod eli;
mod json;
mod runner;

use std::process::ExitCode;

use cli::{Command, Config, ParseOutcome};
use eli::eli_suite;
use runner::{persist_report, run_suite};

fn main() -> ExitCode {
    match Config::parse(std::env::args().skip(1)) {
        Ok(ParseOutcome::Run(config)) => run(config),
        Ok(ParseOutcome::Help(message)) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(config: Config) -> ExitCode {
    let suite = match &config.command {
        Command::BenchEli(eli) => eli_suite(eli),
    };
    if let Some(task_id) = &config.task
        && !suite.tasks.iter().any(|task| task.id == *task_id)
    {
        eprintln!("unknown task for {}: {task_id}", suite.id);
        return ExitCode::from(2);
    }
    let report = run_suite(&suite, &config);
    let report_path = match &config.report_dir {
        Some(root) => match persist_report(&report, root) {
            Ok(path) => Some(path),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(3);
            }
        },
        None => None,
    };
    if config.json {
        println!("{}", report.to_json());
    } else {
        println!("{}", report.to_human(config.debug, report_path.as_deref()));
    }
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
