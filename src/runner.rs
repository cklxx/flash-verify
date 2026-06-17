use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::capture::{CapturedStream, read_bounded_with_needles};
use crate::cli::Config;
use crate::json::{json_bool, json_string};

#[derive(Clone, Debug)]
pub struct Suite {
    pub id: String,
    pub repo: PathBuf,
    pub threshold: u32,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: String,
    pub demand: String,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub timeout: Duration,
    pub threshold: u32,
    pub dependencies: Vec<String>,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub label: String,
    pub points: u32,
    pub hard: bool,
    pub kind: CheckKind,
}

#[derive(Clone, Debug)]
pub enum CheckKind {
    ExitCode(i32),
    StdoutContains(String),
    StderrContains(String),
    AnyOutputForbid(String),
}

#[derive(Debug)]
pub struct RunReport {
    pub suite: String,
    pub repo: PathBuf,
    pub verdict: Verdict,
    pub score: u32,
    pub max_score: u32,
    pub threshold: u32,
    pub duration_ms: u128,
    pub iterations: Vec<IterationReport>,
}

#[derive(Debug)]
pub struct IterationReport {
    pub index: usize,
    pub verdict: Verdict,
    pub score: u32,
    pub max_score: u32,
    pub planned_tasks: usize,
    pub executed_tasks: usize,
    pub tasks: Vec<TaskReport>,
}

#[derive(Debug)]
pub struct TaskReport {
    pub id: String,
    pub demand: String,
    pub verdict: Verdict,
    pub score: u32,
    pub max_score: u32,
    pub threshold: u32,
    pub duration_ms: u128,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub stdout: CapturedStream,
    pub stderr: CapturedStream,
    pub grades: Vec<GradeReport>,
    pub hard_failures: Vec<String>,
}

#[derive(Debug)]
pub struct GradeReport {
    pub label: String,
    pub passed: bool,
    pub hard: bool,
    pub points: u32,
    pub max_points: u32,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Pass,
    Fail,
}

pub fn run_suite(suite: &Suite, config: &Config) -> RunReport {
    let started = Instant::now();
    let tasks = filtered_tasks(suite, config);
    let mut iterations = Vec::new();
    emit_run_event(config, "run_start", suite, 0);
    for index in 1..=config.repeat {
        let iteration = run_iteration(suite, &tasks, config, index);
        let failed = iteration.verdict == Verdict::Fail;
        iterations.push(iteration);
        if failed && !config.keep_going {
            break;
        }
    }
    let report = RunReport::new(suite, started.elapsed(), iterations);
    emit_run_event(config, "run_finish", suite, report.duration_ms);
    report
}

fn filtered_tasks(suite: &Suite, config: &Config) -> Vec<Task> {
    match &config.task {
        Some(task_id) => tasks_with_dependencies(suite, task_id),
        None => suite.tasks.clone(),
    }
}

fn tasks_with_dependencies(suite: &Suite, task_id: &str) -> Vec<Task> {
    let mut tasks = Vec::new();
    let mut seen = Vec::new();
    push_task_with_dependencies(suite, task_id, &mut seen, &mut tasks);
    tasks
}

fn push_task_with_dependencies(
    suite: &Suite,
    task_id: &str,
    seen: &mut Vec<String>,
    tasks: &mut Vec<Task>,
) {
    if seen.iter().any(|id| id == task_id) {
        return;
    }
    let Some(task) = suite.tasks.iter().find(|task| task.id == task_id) else {
        return;
    };
    for dependency in &task.dependencies {
        push_task_with_dependencies(suite, dependency, seen, tasks);
    }
    seen.push(task.id.clone());
    tasks.push(task.clone());
}

fn run_iteration(suite: &Suite, tasks: &[Task], config: &Config, index: usize) -> IterationReport {
    let mut reports = Vec::new();
    for task in tasks {
        emit_task_event(config, "task_start", suite, task, index);
        let report = run_task(task);
        emit_task_event(config, "task_finish", suite, task, index);
        let failed = report.verdict == Verdict::Fail;
        reports.push(report);
        if failed && !config.keep_going {
            break;
        }
    }
    IterationReport::new(index, reports, tasks)
}

fn run_task(task: &Task) -> TaskReport {
    let started = Instant::now();
    let output = spawn_and_wait(task);
    let grades = grade(task, &output);
    let score = grades.iter().map(|grade| grade.points).sum();
    let max_score = grades.iter().map(|grade| grade.max_points).sum();
    let hard_failures = hard_failures(&grades);
    let verdict = verdict(score, max_score, task.threshold, &hard_failures);
    TaskReport {
        id: task.id.clone(),
        demand: task.demand.clone(),
        verdict,
        score,
        max_score,
        threshold: task.threshold,
        duration_ms: started.elapsed().as_millis(),
        argv: task.argv.clone(),
        cwd: task.cwd.clone(),
        timed_out: output.timed_out,
        exit_code: output.exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
        grades,
        hard_failures,
    }
}

struct ChildOutput {
    timed_out: bool,
    exit_code: Option<i32>,
    stdout: CapturedStream,
    stderr: CapturedStream,
}

fn spawn_and_wait(task: &Task) -> ChildOutput {
    let needles = check_needles(task);
    let mut command = Command::new(&task.argv[0]);
    let spawn = command
        .args(&task.argv[1..])
        .current_dir(&task.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(err) => return spawn_error(err.to_string()),
    };
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stdout_needles = needles.clone();
    let stderr_needles = needles;
    let out_thread = thread::spawn(move || read_bounded_with_needles(stdout, &stdout_needles));
    let err_thread = thread::spawn(move || read_bounded_with_needles(stderr, &stderr_needles));
    let (timed_out, exit_code) = wait_with_timeout(&mut child, task.timeout);
    ChildOutput {
        timed_out,
        exit_code,
        stdout: join_stream(out_thread),
        stderr: join_stream(err_thread),
    }
}

fn check_needles(task: &Task) -> Vec<String> {
    let mut needles = Vec::new();
    for check in &task.checks {
        match &check.kind {
            CheckKind::StdoutContains(needle)
            | CheckKind::StderrContains(needle)
            | CheckKind::AnyOutputForbid(needle) => {
                if !needles.iter().any(|known| known == needle) {
                    needles.push(needle.clone());
                }
            }
            CheckKind::ExitCode(_) => {}
        }
    }
    needles
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> (bool, Option<i32>) {
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return (false, status.code());
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let code = child.wait().ok().and_then(|status| status.code());
            return (true, code);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn spawn_error(message: String) -> ChildOutput {
    ChildOutput {
        timed_out: false,
        exit_code: None,
        stdout: empty_stream(),
        stderr: stream_from(message),
    }
}

fn join_stream(handle: thread::JoinHandle<std::io::Result<CapturedStream>>) -> CapturedStream {
    handle
        .join()
        .ok()
        .and_then(Result::ok)
        .unwrap_or_else(|| stream_from("capture failed".into()))
}

fn empty_stream() -> CapturedStream {
    stream_from(String::new())
}

fn stream_from(text: String) -> CapturedStream {
    let bytes = text.len();
    CapturedStream {
        head: text,
        tail: String::new(),
        bytes,
        truncated: false,
        matches: Vec::new(),
    }
}

fn grade(task: &Task, output: &ChildOutput) -> Vec<GradeReport> {
    let mut grades = Vec::new();
    if output.timed_out {
        grades.push(GradeReport::failed("completed before timeout", true, 0));
    }
    grades.extend(task.checks.iter().map(|check| grade_check(check, output)));
    grades
}

fn grade_check(check: &Check, output: &ChildOutput) -> GradeReport {
    let passed = match &check.kind {
        CheckKind::ExitCode(expected) => output.exit_code == Some(*expected),
        CheckKind::StdoutContains(needle) => output.stdout.observed(needle),
        CheckKind::StderrContains(needle) => output.stderr.observed(needle),
        CheckKind::AnyOutputForbid(needle) => {
            !output.stdout.observed(needle) && !output.stderr.observed(needle)
        }
    };
    GradeReport {
        label: check.label.clone(),
        passed,
        hard: check.hard,
        points: if passed { check.points } else { 0 },
        max_points: check.points,
        message: grade_message(check, passed, output),
    }
}

fn grade_message(check: &Check, passed: bool, output: &ChildOutput) -> String {
    if passed {
        return "passed".into();
    }
    match &check.kind {
        CheckKind::ExitCode(expected) => {
            format!("expected exit code {expected}, got {:?}", output.exit_code)
        }
        CheckKind::StdoutContains(needle) => format!("stdout missing {needle:?}"),
        CheckKind::StderrContains(needle) => format!("stderr missing {needle:?}"),
        CheckKind::AnyOutputForbid(needle) => format!("output contains forbidden {needle:?}"),
    }
}

fn hard_failures(grades: &[GradeReport]) -> Vec<String> {
    grades
        .iter()
        .filter(|grade| grade.hard && !grade.passed)
        .map(|grade| grade.message.clone())
        .collect()
}

fn verdict(score: u32, max_score: u32, threshold: u32, hard_failures: &[String]) -> Verdict {
    if !hard_failures.is_empty() {
        return Verdict::Fail;
    }
    if max_score == 0 || score * 100 / max_score >= threshold {
        Verdict::Pass
    } else {
        Verdict::Fail
    }
}

fn emit_run_event(config: &Config, event: &str, suite: &Suite, duration_ms: u128) {
    if config.events {
        eprintln!(
            "{{\"event\":{},\"suite\":{},\"duration_ms\":{}}}",
            json_string(event),
            json_string(&suite.id),
            duration_ms
        );
    }
}

fn emit_task_event(config: &Config, event: &str, suite: &Suite, task: &Task, iteration: usize) {
    if config.events {
        eprintln!(
            "{{\"event\":{},\"suite\":{},\"iteration\":{},\"task\":{}}}",
            json_string(event),
            json_string(&suite.id),
            iteration,
            json_string(&task.id)
        );
    }
}

pub fn persist_report(report: &RunReport, root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(root).map_err(|err| format!("create report root: {err}"))?;
    let dir = create_unique_report_dir(root, report)?;
    fs::write(dir.join("summary.json"), report.to_json()).map_err(|err| err.to_string())?;
    for iteration in &report.iterations {
        persist_iteration(&dir, iteration)?;
    }
    Ok(dir)
}

fn create_unique_report_dir(root: &Path, report: &RunReport) -> Result<PathBuf, String> {
    let stem = format!(
        "{}-{}-{}",
        unix_millis(),
        std::process::id(),
        sanitize_path(&report.suite)
    );
    for attempt in 0..1000 {
        let name = if attempt == 0 {
            stem.clone()
        } else {
            format!("{stem}-{attempt}")
        };
        let dir = root.join(name);
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(format!("create report dir: {err}")),
        }
    }
    Err("create report dir: exhausted unique names".into())
}

fn persist_iteration(root: &Path, iteration: &IterationReport) -> Result<(), String> {
    for task in &iteration.tasks {
        let dir = root
            .join(format!("iteration-{}", iteration.index))
            .join(sanitize_path(&task.id));
        fs::create_dir_all(&dir).map_err(|err| format!("create task dir: {err}"))?;
        fs::write(dir.join("stdout.txt"), task.stdout.as_text()).map_err(|err| err.to_string())?;
        fs::write(dir.join("stderr.txt"), task.stderr.as_text()).map_err(|err| err.to_string())?;
        fs::write(dir.join("command.txt"), task.argv.join("\n")).map_err(|err| err.to_string())?;
        fs::write(dir.join("grade.json"), task.to_json()).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn sanitize_path(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

impl CapturedStream {
    pub fn as_text(&self) -> String {
        if self.truncated {
            format!("{}\n...<truncated>...\n{}", self.head, self.tail)
        } else {
            self.head.clone()
        }
    }

    fn observed(&self, needle: &str) -> bool {
        self.matches.iter().any(|matched| matched == needle) || self.as_text().contains(needle)
    }
}

impl GradeReport {
    fn failed(label: &str, hard: bool, max_points: u32) -> Self {
        Self {
            label: label.into(),
            passed: false,
            hard,
            points: 0,
            max_points,
            message: label.into(),
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"label\":{},\"passed\":{},\"hard\":{},\"points\":{},\"max_points\":{},\"message\":{}}}",
            json_string(&self.label),
            json_bool(self.passed),
            json_bool(self.hard),
            self.points,
            self.max_points,
            json_string(&self.message)
        )
    }
}

impl RunReport {
    fn new(suite: &Suite, duration: Duration, iterations: Vec<IterationReport>) -> Self {
        let score = iterations.iter().map(|iteration| iteration.score).sum();
        let max_score = iterations.iter().map(|iteration| iteration.max_score).sum();
        let hard_failures = iterations
            .iter()
            .flat_map(|iteration| iteration.hard_failures())
            .collect::<Vec<_>>();
        Self {
            suite: suite.id.clone(),
            repo: suite.repo.clone(),
            verdict: verdict(score, max_score, suite.threshold, &hard_failures),
            score,
            max_score,
            threshold: suite.threshold,
            duration_ms: duration.as_millis(),
            iterations,
        }
    }

    pub fn passed(&self) -> bool {
        self.verdict == Verdict::Pass
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema\":\"fv/bench/v1\",\"suite\":{},\"repo\":{},\"verdict\":{},\"score\":{},\"max_score\":{},\"threshold\":{},\"duration_ms\":{},\"iterations\":[{}]}}",
            json_string(&self.suite),
            json_string(&self.repo.display().to_string()),
            self.verdict.to_json(),
            self.score,
            self.max_score,
            self.threshold,
            self.duration_ms,
            self.iterations
                .iter()
                .map(IterationReport::to_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn to_human(&self, debug: bool, report_path: Option<&Path>) -> String {
        let mut lines = vec![
            format!("flash-verify {}", self.suite),
            format!("repo: {}", self.repo.display()),
            format!("threshold: {}", self.threshold),
            String::new(),
        ];
        for iteration in &self.iterations {
            lines.push(format!("iteration {}", iteration.index));
            lines.extend(
                iteration
                    .tasks
                    .iter()
                    .flat_map(|task| task.human_lines(debug)),
            );
        }
        lines.push(String::new());
        lines.push(format!("verdict: {}", self.verdict.as_str()));
        lines.push(format!("score: {}/{}", self.score, self.max_score));
        lines.push(format!("duration_ms: {}", self.duration_ms));
        if let Some(path) = report_path {
            lines.push(format!("report: {}", path.display()));
        }
        lines.join("\n")
    }
}

impl IterationReport {
    fn new(index: usize, tasks: Vec<TaskReport>, planned_tasks: &[Task]) -> Self {
        let score = tasks.iter().map(|task| task.score).sum();
        let max_score = planned_tasks.iter().map(Task::max_score).sum();
        let hard_failures = tasks
            .iter()
            .flat_map(|task| task.hard_failures.clone())
            .collect::<Vec<_>>();
        let executed_tasks = tasks.len();
        Self {
            index,
            verdict: verdict(score, max_score, 100, &hard_failures),
            score,
            max_score,
            planned_tasks: planned_tasks.len(),
            executed_tasks,
            tasks,
        }
    }

    fn hard_failures(&self) -> Vec<String> {
        self.tasks
            .iter()
            .flat_map(|task| task.hard_failures.clone())
            .collect()
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"index\":{},\"verdict\":{},\"score\":{},\"max_score\":{},\"planned_tasks\":{},\"executed_tasks\":{},\"tasks\":[{}]}}",
            self.index,
            self.verdict.to_json(),
            self.score,
            self.max_score,
            self.planned_tasks,
            self.executed_tasks,
            self.tasks
                .iter()
                .map(TaskReport::to_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl Task {
    fn max_score(&self) -> u32 {
        self.checks.iter().map(|check| check.points).sum()
    }
}

impl TaskReport {
    fn human_lines(&self, debug: bool) -> Vec<String> {
        let mut lines = vec![format!(
            "{:<4} {:<24} {:>4}/{:<4} {:>6}ms  {}",
            self.verdict.as_str(),
            self.id,
            self.score,
            self.max_score,
            self.duration_ms,
            self.demand
        )];
        for failure in &self.hard_failures {
            lines.push(format!("      hard gate: {failure}"));
        }
        if debug && self.verdict == Verdict::Fail {
            lines.push(format!("      stdout: {}", snippet(&self.stdout.as_text())));
            lines.push(format!("      stderr: {}", snippet(&self.stderr.as_text())));
        }
        lines
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"id\":{},\"demand\":{},\"verdict\":{},\"score\":{},\"max_score\":{},\"threshold\":{},\"duration_ms\":{},\"argv\":[{}],\"cwd\":{},\"timed_out\":{},\"exit_code\":{},\"stdout\":{},\"stderr\":{},\"hard_failures\":[{}],\"grades\":[{}]}}",
            json_string(&self.id),
            json_string(&self.demand),
            self.verdict.to_json(),
            self.score,
            self.max_score,
            self.threshold,
            self.duration_ms,
            self.argv
                .iter()
                .map(|arg| json_string(arg))
                .collect::<Vec<_>>()
                .join(","),
            json_string(&self.cwd.display().to_string()),
            json_bool(self.timed_out),
            self.exit_code
                .map_or("null".into(), |code| code.to_string()),
            stream_json(&self.stdout),
            stream_json(&self.stderr),
            self.hard_failures
                .iter()
                .map(|failure| json_string(failure))
                .collect::<Vec<_>>()
                .join(","),
            self.grades
                .iter()
                .map(GradeReport::to_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
        }
    }

    fn to_json(self) -> String {
        json_string(self.as_str())
    }
}

fn stream_json(stream: &CapturedStream) -> String {
    format!(
        "{{\"head\":{},\"tail\":{},\"bytes\":{},\"truncated\":{},\"matches\":[{}]}}",
        json_string(&stream.head),
        json_string(&stream.tail),
        stream.bytes,
        json_bool(stream.truncated),
        stream
            .matches
            .iter()
            .map(|needle| json_string(needle))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn snippet(text: &str) -> String {
    text.lines().take(3).collect::<Vec<_>>().join("\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_task_passes_exit_gate() {
        let task = Task {
            id: "true".into(),
            demand: "true exits successfully".into(),
            cwd: PathBuf::from("."),
            argv: vec!["/usr/bin/true".into()],
            timeout: Duration::from_secs(1),
            threshold: 100,
            dependencies: Vec::new(),
            checks: vec![Check::hard("exit code is zero", 1, CheckKind::ExitCode(0))],
        };
        assert_eq!(run_task(&task).verdict, Verdict::Pass);
    }

    #[test]
    fn selected_task_includes_dependencies_first() {
        let dependency = Task {
            id: "build".into(),
            demand: "build binary".into(),
            cwd: PathBuf::from("."),
            argv: vec!["/usr/bin/true".into()],
            timeout: Duration::from_secs(1),
            threshold: 100,
            dependencies: Vec::new(),
            checks: vec![],
        };
        let target = Task {
            id: "cli-help".into(),
            demand: "run help".into(),
            cwd: PathBuf::from("."),
            argv: vec!["/usr/bin/true".into()],
            timeout: Duration::from_secs(1),
            threshold: 100,
            dependencies: vec!["build".into()],
            checks: vec![],
        };
        let suite = Suite {
            id: "test".into(),
            repo: PathBuf::from("."),
            threshold: 100,
            tasks: vec![target, dependency],
        };
        let tasks = tasks_with_dependencies(&suite, "cli-help");
        let ids = tasks.into_iter().map(|task| task.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["build", "cli-help"]);
    }

    #[test]
    fn forbid_check_uses_observed_matches() {
        let output = ChildOutput {
            timed_out: false,
            exit_code: Some(0),
            stdout: CapturedStream {
                head: "start".into(),
                tail: "end".into(),
                bytes: 100_000,
                truncated: true,
                matches: vec!["panic".into()],
            },
            stderr: CapturedStream {
                head: String::new(),
                tail: String::new(),
                bytes: 0,
                truncated: false,
                matches: Vec::new(),
            },
        };
        let check = Check::hard("no panic", 1, CheckKind::AnyOutputForbid("panic".into()));
        let grade = grade_check(&check, &output);
        assert!(!grade.passed);
    }
}

impl Check {
    pub fn hard(label: &str, points: u32, kind: CheckKind) -> Self {
        Self {
            label: label.into(),
            points,
            hard: true,
            kind,
        }
    }

    pub fn soft(label: &str, points: u32, kind: CheckKind) -> Self {
        Self {
            label: label.into(),
            points,
            hard: false,
            kind,
        }
    }
}
