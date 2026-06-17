use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub command: Command,
    pub keep_going: bool,
    pub events: bool,
    pub repeat: usize,
    pub json: bool,
    pub debug: bool,
    pub report_dir: Option<PathBuf>,
    pub task: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ParseOutcome {
    Run(Config),
    Help(String),
}

#[derive(Clone, Debug)]
pub enum Command {
    BenchEli(EliConfig),
}

#[derive(Clone, Debug)]
pub struct EliConfig {
    pub repo: PathBuf,
    pub suite: EliSuite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EliSuite {
    Quick,
    CliSmoke,
    Smoke,
    Rust,
    Full,
    SidecarFeishu,
    GatewayCore,
    HardTail,
}

impl Config {
    pub fn parse(args: impl Iterator<Item = String>) -> Result<ParseOutcome, String> {
        let mut args = args.peekable();
        match args.next().as_deref() {
            Some("bench") => parse_bench(args),
            Some("-h" | "--help") | None => Ok(ParseOutcome::Help(usage())),
            Some(other) => Err(format!("unknown command: {other}\n\n{}", usage())),
        }
    }
}

fn parse_bench(mut args: impl Iterator<Item = String>) -> Result<ParseOutcome, String> {
    match args.next().as_deref() {
        Some("eli") => parse_eli(args),
        Some(other) => Err(format!("unknown bench target: {other}\n\n{}", usage())),
        None => Ok(ParseOutcome::Help(usage())),
    }
}

fn parse_eli(args: impl Iterator<Item = String>) -> Result<ParseOutcome, String> {
    let mut opts = EliOptions::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => opts.repo = PathBuf::from(next_value(&mut args, "--repo")?),
            "--suite" => opts.suite = parse_suite(&next_value(&mut args, "--suite")?)?,
            "--task" => opts.task = Some(next_value(&mut args, "--task")?),
            "--keep-going" => opts.keep_going = true,
            "--events" => opts.events = true,
            "--json" => opts.json = true,
            "--debug" => opts.debug = true,
            "--report" => opts.report_dir = Some(PathBuf::from(next_value(&mut args, "--report")?)),
            "--repeat" => opts.repeat = parse_repeat(&next_value(&mut args, "--repeat")?)?,
            "-h" | "--help" => return Ok(ParseOutcome::Help(eli_usage())),
            other => return Err(format!("unknown eli option: {other}\n\n{}", eli_usage())),
        }
    }
    Ok(ParseOutcome::Run(opts.into_config()))
}

#[derive(Debug)]
struct EliOptions {
    repo: PathBuf,
    suite: EliSuite,
    keep_going: bool,
    events: bool,
    repeat: usize,
    json: bool,
    debug: bool,
    report_dir: Option<PathBuf>,
    task: Option<String>,
}

impl Default for EliOptions {
    fn default() -> Self {
        Self {
            repo: PathBuf::from("../eli"),
            suite: EliSuite::Quick,
            keep_going: false,
            events: false,
            repeat: 1,
            json: false,
            debug: false,
            report_dir: None,
            task: None,
        }
    }
}

impl EliOptions {
    fn into_config(self) -> Config {
        Config {
            command: Command::BenchEli(EliConfig {
                repo: self.repo,
                suite: self.suite,
            }),
            keep_going: self.keep_going,
            events: self.events,
            repeat: self.repeat,
            json: self.json,
            debug: self.debug,
            report_dir: self.report_dir,
            task: self.task,
        }
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_suite(value: &str) -> Result<EliSuite, String> {
    match value {
        "quick" => Ok(EliSuite::Quick),
        "cli-smoke" => Ok(EliSuite::CliSmoke),
        "smoke" => Ok(EliSuite::Smoke),
        "rust" => Ok(EliSuite::Rust),
        "full" => Ok(EliSuite::Full),
        "sidecar-feishu" => Ok(EliSuite::SidecarFeishu),
        "gateway-core" => Ok(EliSuite::GatewayCore),
        "hard-tail" => Ok(EliSuite::HardTail),
        _ => Err(format!("unknown eli suite: {value}")),
    }
}

fn parse_repeat(value: &str) -> Result<usize, String> {
    let repeat = value
        .parse()
        .map_err(|_| format!("invalid --repeat: {value}"))?;
    if repeat == 0 {
        return Err("--repeat must be positive".into());
    }
    Ok(repeat)
}

fn usage() -> String {
    "usage: flash-verify bench eli [--repo PATH] [--suite quick|cli-smoke|smoke|rust|full|sidecar-feishu|gateway-core|hard-tail] [--task ID] [--repeat N] [--keep-going] [--events] [--json] [--debug] [--report DIR]".into()
}

fn eli_usage() -> String {
    "usage: flash-verify bench eli [--repo ../eli] [--suite quick|cli-smoke|smoke|rust|full|sidecar-feishu|gateway-core|hard-tail] [--task ID] [--repeat N] [--keep-going] [--events] [--json] [--debug] [--report DIR]".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_eli_suite_targets_neighbor_repo() {
        let ParseOutcome::Run(config) =
            Config::parse(["bench", "eli"].into_iter().map(String::from)).unwrap()
        else {
            panic!("expected run config");
        };
        let Command::BenchEli(eli) = config.command;
        assert_eq!(eli.repo, PathBuf::from("../eli"));
        assert_eq!(eli.suite, EliSuite::Quick);
    }

    #[test]
    fn repeat_must_be_positive() {
        let err = Config::parse(
            ["bench", "eli", "--repeat", "0"]
                .into_iter()
                .map(String::from),
        )
        .unwrap_err();
        assert!(err.contains("positive"));
    }

    #[test]
    fn top_level_eli_alias_is_rejected() {
        let err = Config::parse(["eli"].into_iter().map(String::from)).unwrap_err();
        assert!(err.contains("unknown command"));
    }

    #[test]
    fn help_is_not_an_invalid_config_error() {
        let ParseOutcome::Help(message) =
            Config::parse(["bench", "eli", "--help"].into_iter().map(String::from)).unwrap()
        else {
            panic!("expected help");
        };
        assert!(message.contains("flash-verify bench eli"));
    }
}
