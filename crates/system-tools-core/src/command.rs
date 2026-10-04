use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PRIVILEGED_COMMAND_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const SUDO_SEARCH_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const DEBUG_TIMINGS_ENV: &str = "SYSTEM_TOOLS_DEBUG_TIMINGS";

const UNSAFE_ENVIRONMENT: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "PYTHONPATH",
    "PYTHONHOME",
    "RUBYLIB",
    "PERL5LIB",
    "BASH_ENV",
    "ENV",
];

fn scrub_privileged_environment(command: &mut Command) {
    command.env("PATH", PRIVILEGED_COMMAND_PATH);
    command.env("LC_ALL", "C.UTF-8");
    command.env_remove("LANGUAGE");
    for variable in UNSAFE_ENVIRONMENT {
        command.env_remove(variable);
    }
}

fn debug_timings_enabled(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true" | "yes"))
}

fn debug_timing_line(program: &OsStr, elapsed: Duration, status: ExitStatus) -> String {
    format!(
        "command_timing program={} elapsed_ms={} status={}",
        program.to_string_lossy(),
        elapsed.as_millis(),
        status
    )
}

fn report_debug_timing(program: &OsStr, started: Instant, status: ExitStatus) {
    if debug_timings_enabled(std::env::var(DEBUG_TIMINGS_ENV).ok().as_deref()) {
        eprintln!("{}", debug_timing_line(program, started.elapsed(), status));
    }
}

pub struct PrivilegeRunner {
    sudo_path: PathBuf,
}

#[cfg(test)]
thread_local! {
    pub(crate) static TEST_SUDO_PATH: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

impl PrivilegeRunner {
    pub fn system() -> Result<Self> {
        #[cfg(test)]
        if let Some(sudo_path) = TEST_SUDO_PATH.with_borrow(Clone::clone) {
            return Ok(Self { sudo_path });
        }
        Self::from_search_path(OsStr::new(SUDO_SEARCH_PATH))
    }

    fn from_search_path(search_path: &OsStr) -> Result<Self> {
        let resolver = crate::ExecutableResolver::from_path(Some(search_path));
        let sudo_path = resolver
            .resolve(OsStr::new("sudo"))
            .context("sudo not found in trusted system path")?;
        Ok(Self { sudo_path })
    }

    pub fn run<S>(&self, args: &[S]) -> Result<ExitStatus>
    where
        S: AsRef<OsStr>,
    {
        let started = Instant::now();
        let mut command = Command::new(&self.sudo_path);
        command.args(args);
        scrub_privileged_environment(&mut command);
        let status = command.status().context("failed to execute sudo")?;
        report_debug_timing(OsStr::new("sudo"), started, status);
        if !status.success() {
            bail!("sudo exited with {status}");
        }
        Ok(status)
    }

    pub fn run_plan(&self, plan: &crate::CommandPlan) -> Result<ExitStatus> {
        let started = Instant::now();
        let mut command = Command::new(&self.sudo_path);
        command.arg(&plan.program).args(&plan.args);
        scrub_privileged_environment(&mut command);
        let status = command
            .apply_plan_environment(plan)
            .status()
            .context("failed to execute sudo")?;
        report_debug_timing(OsStr::new("sudo"), started, status);
        if !status.success() {
            bail!("sudo exited with {status}");
        }
        Ok(status)
    }

    fn capture_plan(&self, plan: &crate::CommandPlan) -> Result<Output> {
        let mut command = Command::new(&self.sudo_path);
        command.arg(&plan.program).args(&plan.args);
        scrub_privileged_environment(&mut command);
        capture_command(command.apply_plan_environment(plan), true)
    }
}

trait CommandEnvironment {
    fn apply_plan_environment(&mut self, plan: &crate::CommandPlan) -> &mut Self;
}

impl CommandEnvironment for Command {
    fn apply_plan_environment(&mut self, plan: &crate::CommandPlan) -> &mut Self {
        for variable in &plan.env_remove {
            self.env_remove(variable);
        }
        if let Some(locale) = &plan.locale {
            self.env("LC_ALL", locale);
        }
        self
    }
}

pub fn require_privileged() -> Result<()> {
    PrivilegeRunner::system().map(|_| ())
}

pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn require_command(command: &str) -> Result<()> {
    if crate::command_exists(command) {
        Ok(())
    } else {
        bail!("required command not found: {command}")
    }
}

pub fn require_command_for(command: &str, capability: &str, optional: bool) -> Result<()> {
    resolve_command_for(command, capability, optional).map(|_| ())
}

pub fn resolve_command_for(command: &str, capability: &str, optional: bool) -> Result<PathBuf> {
    let resolver = crate::ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    if let Some(path) = resolver.resolve(OsStr::new(command)) {
        return Ok(path);
    }
    if optional {
        bail!("optional command '{command}' is unavailable; {capability} will be skipped")
    } else {
        bail!("required command '{command}' is unavailable for {capability}; install it and retry")
    }
}

pub fn run_capture<S>(program: &str, args: &[S], allow_failure: bool) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    let started = Instant::now();
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    let result = Output {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    report_debug_timing(OsStr::new(program), started, result.status);
    if !allow_failure && !result.status.success() {
        bail!(
            "{program} exited with {}: {}",
            result.status,
            result.stderr.trim()
        );
    }
    Ok(result)
}

pub fn run_capture_path<S>(program: &Path, args: &[S], allow_failure: bool) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    run_capture_os(program.as_os_str(), args, allow_failure)
}

fn run_capture_os<S>(program: &OsStr, args: &[S], allow_failure: bool) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    capture_command(Command::new(program).args(args), allow_failure)
}

fn capture_command(command: &mut Command, allow_failure: bool) -> Result<Output> {
    let started = Instant::now();
    let output = command.output().with_context(|| {
        format!(
            "failed to execute {}",
            command.get_program().to_string_lossy()
        )
    })?;
    let program = command.get_program();
    let result = Output {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    report_debug_timing(program, started, result.status);
    if !allow_failure && !result.status.success() {
        bail!(
            "{} exited with {}: {}",
            program.to_string_lossy(),
            result.status,
            result.stderr.trim()
        );
    }
    Ok(result)
}

pub(crate) fn run_capture_plan(plan: &crate::CommandPlan) -> Result<Output> {
    capture_plan_with_runner(plan, PrivilegeRunner::system)
}

fn capture_plan_with_runner(
    plan: &crate::CommandPlan,
    privilege_runner: impl FnOnce() -> Result<PrivilegeRunner>,
) -> Result<Output> {
    match plan.privilege {
        crate::CommandPrivilege::User => capture_command(
            Command::new(&plan.program)
                .args(&plan.args)
                .apply_plan_environment(plan),
            true,
        ),
        crate::CommandPrivilege::Elevated => privilege_runner()?.capture_plan(plan),
    }
}

pub fn run_capture_no_args(program: &str, allow_failure: bool) -> Result<Output> {
    run_capture(program, &[] as &[&str], allow_failure)
}

pub fn run_status<S>(program: &str, args: &[S]) -> Result<ExitStatus>
where
    S: AsRef<OsStr>,
{
    let started = Instant::now();
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute {program}"))?;
    report_debug_timing(OsStr::new(program), started, status);
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(status)
}

pub fn run_status_path<S>(program: &Path, args: &[S]) -> Result<ExitStatus>
where
    S: AsRef<OsStr>,
{
    let started = Instant::now();
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute {}", program.to_string_lossy()))?;
    report_debug_timing(program.as_os_str(), started, status);
    if !status.success() {
        bail!("{} exited with {status}", program.to_string_lossy());
    }
    Ok(status)
}

pub fn run_status_path_timeout<S>(
    program: &Path,
    args: &[S],
    timeout: Duration,
) -> Result<ExitStatus>
where
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to execute {}", program.display()))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            report_debug_timing(program.as_os_str(), started, status);
            if !status.success() {
                bail!("{} exited with {status}", program.display());
            }
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "{} timed out after {}s",
                program.display(),
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn run_command_plan(plan: &crate::CommandPlan) -> Result<ExitStatus> {
    match plan.privilege {
        crate::CommandPrivilege::User => {
            let started = Instant::now();
            let status = Command::new(&plan.program)
                .args(&plan.args)
                .apply_plan_environment(plan)
                .status()
                .with_context(|| format!("failed to execute {}", plan.program.display()))?;
            report_debug_timing(plan.program.as_os_str(), started, status);
            if !status.success() {
                bail!("{} exited with {status}", plan.program.display());
            }
            Ok(status)
        }
        crate::CommandPrivilege::Elevated => PrivilegeRunner::system()?.run_plan(plan),
    }
}

pub fn run_privileged<S>(args: &[S]) -> Result<ExitStatus>
where
    S: AsRef<OsStr>,
{
    PrivilegeRunner::system()?.run(args)
}

pub fn run_privileged_quiet<S>(args: &[S]) -> Result<ExitStatus>
where
    S: AsRef<OsStr>,
{
    let runner = PrivilegeRunner::system()?;
    let mut command = Command::new(&runner.sudo_path);
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    scrub_privileged_environment(&mut command);
    let status = command.status().context("failed to execute sudo")?;
    if !status.success() {
        bail!("sudo exited with {status}");
    }
    Ok(status)
}

pub fn run_status_no_args<S>(program: S) -> Result<ExitStatus>
where
    S: AsRef<OsStr>,
{
    let program_ref = program.as_ref();
    let started = Instant::now();
    let status = Command::new(program_ref)
        .status()
        .with_context(|| format!("failed to execute {}", program_ref.to_string_lossy()))?;
    report_debug_timing(program_ref, started, status);
    if !status.success() {
        bail!("{} exited with {status}", program_ref.to_string_lossy());
    }
    Ok(status)
}

#[cfg(test)]
#[path = "command/tests.rs"]
mod tests;
