use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, ExitStatus};
use std::time::Instant;
use system_tools_core::{
    ExecutableResolver, current_executable, require_command_for, run_capture_path,
};

const COMMON_FZF_LAYOUT_ARGS: &[&str] = &[
    "--ansi",
    "--no-wrap",
    "--no-hscroll",
    "--ellipsis=...",
    "--layout=reverse",
    "--info=inline",
    "--border",
    "--height=95%",
    "--tiebreak=index",
];

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn classify_fzf_status(status: ExitStatus) -> Result<bool> {
    if status.success() {
        return Ok(true);
    }
    match status.code() {
        Some(1 | 130) => Ok(false),
        Some(code) => bail!("fzf exited with status {code}"),
        None => bail!("fzf was terminated by a signal"),
    }
}

pub(crate) fn collect_update_rows() -> Result<String> {
    let mut rows = String::new();
    let mut seen = HashSet::new();
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let checkupdates = resolver.resolve(OsStr::new("checkupdates"));
    let helper = resolver
        .resolve(OsStr::new("paru"))
        .or_else(|| resolver.resolve(OsStr::new("yay")));
    let flatpak = resolver.resolve(OsStr::new("flatpak"));
    let (repo, aur, flatpak) = std::thread::scope(|scope| -> Result<_> {
        let repo = scope.spawn(move || {
            checkupdates.and_then(|program| {
                run_capture_path(&program, &[] as &[&str], true)
                    .ok()
                    .map(|output| output.stdout)
            })
        });
        let aur = scope.spawn(move || {
            helper.and_then(|program| {
                run_capture_path(&program, &["-Qua"], true)
                    .ok()
                    .map(|output| output.stdout)
            })
        });
        let flatpak = scope.spawn(move || {
            flatpak.and_then(|program| {
                run_capture_path(
                    &program,
                    &["remote-ls", "--updates", "--columns=application,version"],
                    true,
                )
                .ok()
                .map(|output| output.stdout)
            })
        });
        Ok((
            repo.join()
                .map_err(|_| anyhow::anyhow!("repository update query thread panicked"))?,
            aur.join()
                .map_err(|_| anyhow::anyhow!("AUR update query thread panicked"))?,
            flatpak
                .join()
                .map_err(|_| anyhow::anyhow!("Flatpak update query thread panicked"))?,
        ))
    })?;
    if let Some(output) = repo {
        append_source_rows(&mut rows, &mut seen, "pacman", "34", &output);
    }
    if let Some(output) = aur {
        append_source_rows(&mut rows, &mut seen, "aur", "35", &output);
    }
    if let Some(output) = flatpak {
        append_source_rows(&mut rows, &mut seen, "flatpak", "36", &output);
    }
    Ok(rows)
}

fn append_source_rows(
    rows: &mut String,
    seen: &mut HashSet<String>,
    source: &str,
    color: &str,
    output: &str,
) {
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let name = line.split_whitespace().next().unwrap_or(line);
        if !seen.insert(name.to_owned()) {
            continue;
        }
        if !rows.is_empty() {
            rows.push('\n');
        }
        rows.push_str("\x1b[");
        rows.push_str(color);
        rows.push_str("m[");
        rows.push_str(source);
        rows.push_str("]\x1b[0m\t");
        rows.push_str(name);
        rows.push('\t');
        rows.push_str(line);
    }
}

pub(crate) fn show_update_list() -> Result<bool> {
    let picker_started = Instant::now();
    require_command_for("fzf", "the system update list picker", false)?;
    let rows = collect_update_rows()?;
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "picker_timing picker=systide-list phase=prepared elapsed_ms={}",
            picker_started.elapsed().as_millis()
        );
    }
    let executable = current_executable()?;
    let reload = format!(
        "{} --list-data",
        shell_quote(executable.to_string_lossy().as_ref())
    );
    let bind = format!(
        "ctrl-r:change-prompt(正在拉取最新数据... > )+reload-sync({reload})+change-prompt(待更新项目 > )"
    );
    let mut args = COMMON_FZF_LAYOUT_ARGS.to_vec();
    args.extend([
        "--delimiter=\t",
        "--nth=2",
        "--id-nth=2",
        "--track",
        "--prompt",
        "待更新项目 > ",
        "--header=ESC退出 | Enter更新系统 | Ctrl+R强制刷新",
        "--bind",
        bind.as_str(),
    ]);
    let mut child = Command::new("fzf")
        .args(args)
        .env("FZF_DEFAULT_OPTS", "")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .context("cannot start update list")?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(rows.as_bytes())?;
    }
    let status = child.wait()?;
    classify_fzf_status(status)
        .map_err(|error| anyhow::anyhow!("system update list picker failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::classify_fzf_status;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn classifies_accept_cancel_and_failure_statuses() {
        assert!(classify_fzf_status(status(0)).expect("accepted status"));
        assert!(!classify_fzf_status(status(1)).expect("cancel status"));
        assert!(!classify_fzf_status(status(130)).expect("interrupt status"));
        assert!(classify_fzf_status(status(2)).is_err());
        assert!(classify_fzf_status(ExitStatus::from_raw(9)).is_err());
    }
}
