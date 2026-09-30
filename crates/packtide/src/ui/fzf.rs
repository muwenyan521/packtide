use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{BufWriter, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use system_tools_core::current_executable;
use unicode_width::UnicodeWidthStr;

use crate::ui::COMMON_FZF_LAYOUT_ARGS;

use super::preview::shell_quote;

pub(crate) fn classify_fzf_status(status: ExitStatus) -> Result<bool> {
    if status.success() {
        return Ok(true);
    }
    match status.code() {
        Some(1 | 130) => Ok(false),
        Some(code) => bail!("fzf exited with status {code}"),
        None => bail!("fzf was terminated by a signal"),
    }
}

pub(crate) fn classify_picker_status(status: ExitStatus, picker: &str) -> Result<bool> {
    classify_fzf_status(status).map_err(|error| anyhow::anyhow!("{picker} picker failed: {error}"))
}

pub(crate) fn select_rows(
    helper: &str,
    removing: bool,
    rows: String,
    query: &[String],
    started: Option<SystemTime>,
) -> Result<Option<String>> {
    if rows.is_empty() {
        return Ok(Some(String::new()));
    }
    select_rows_with_input(helper, removing, query, started, move |stdin| {
        stdin.write_all(rows.as_bytes())?;
        Ok(())
    })
}

pub(crate) fn select_install_catalog_streaming(
    helper: &str,
    pacman: &std::path::Path,
    refresh: bool,
    query: &[String],
    started: Option<SystemTime>,
) -> Result<Option<String>> {
    let pacman = pacman.to_path_buf();
    select_rows_with_input(helper, false, query, started, move |stdin| {
        let source_started = Instant::now();
        let mut official_first = false;
        let mut rows_started = false;
        let catalog = crate::sources::install_rows_streaming(&pacman, refresh, false, |record| {
            if !official_first {
                timing_event("official_first", source_started);
                official_first = true;
            }
            if rows_started {
                stdin.write_all(b"\n")?;
            }
            super::rows::write_package_row(stdin, record, super::rows::PackageListMode::Install)
                .context("failed writing package row to fzf")?;
            rows_started = true;
            Ok(())
        })?;
        timing_event("official_done", source_started);
        timing_event("aur_start", source_started);
        super::rows::write_aur_install_rows(&catalog, stdin, rows_started)
            .context("failed writing AUR rows to fzf")?;
        timing_event("aur_done", source_started);
        Ok(())
    })
}

fn timing_event(phase: &str, started: Instant) {
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "picker_timing picker=install phase={phase} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
}

fn select_rows_with_input(
    helper: &str,
    removing: bool,
    query: &[String],
    started: Option<SystemTime>,
    write_input: impl FnOnce(&mut dyn Write) -> Result<()> + Send + 'static,
) -> Result<Option<String>> {
    let executable = current_executable()?;
    let mode = if removing { "remove" } else { "install" };
    let lang = crate::locale::current();
    let reload_command = format!(
        "PACKTIDE_{}_LIST_ONLY=1 {} {}{}",
        if removing { "REMOVE" } else { "INSTALL" },
        shell_quote(executable.to_string_lossy().as_ref()),
        mode,
        if removing { "" } else { " --refresh" }
    );
    let reload = format!(
        "ctrl-r:change-prompt({})+reload-sync({reload_command})+change-prompt({})",
        crate::locale::text(
            lang,
            if removing {
                "remove.refresh"
            } else {
                "install.refresh"
            },
            &[],
        ),
        crate::locale::text(
            lang,
            if removing {
                "remove.prompt"
            } else {
                "install.prompt"
            },
            &[]
        )
    );
    let action = if removing {
        crate::locale::text(lang, "remove.actions", &[])
    } else {
        crate::locale::text(lang, "install.actions", &[])
    };
    let title = crate::locale::text(
        lang,
        if removing {
            "remove.title"
        } else {
            "install.title"
        },
        &[],
    );
    let prompt = crate::locale::text(
        lang,
        if removing {
            "remove.prompt"
        } else {
            "install.prompt"
        },
        &[],
    );
    let refresh = crate::locale::text(lang, "picker.refresh", &[]);
    let exit = crate::locale::text(lang, "picker.exit", &[]);
    let using = crate::locale::text(lang, "picker.using", &[("helper", helper)]);
    let columns = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(80);
    let title_width = UnicodeWidthStr::width(title.as_str());
    let using_width = UnicodeWidthStr::width(using.as_str());
    let gap = columns.saturating_sub(title_width + using_width).max(2);
    let action_line = format!("{action} | {refresh} | {exit}");
    let action_line =
        if UnicodeWidthStr::width(action_line.as_str()) + title_width + using_width + 2 > columns {
            action.to_owned()
        } else {
            action_line
        };
    let title_color = if removing { "1;33" } else { "1;36" };
    let yellow_using = format!("\x1b[33m{using}\x1b[0m");
    let header = format!(
        "\x1b[{title_color}m{title}\x1b[0m{}{yellow_using}\n\x1b[2m{action_line}\x1b[0m",
        " ".repeat(gap)
    );
    let marker = if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Some(std::env::temp_dir().join(format!("packtide-ready-{}-{nonce}", std::process::id())))
    } else {
        None
    };
    let timing_bind = marker.as_ref().map(|marker| {
        let mark_command = format!(
            "{} __timing-mark {}",
            shell_quote(executable.to_string_lossy().as_ref()),
            shell_quote(marker.to_string_lossy().as_ref())
        );
        format!("load:execute-silent({mark_command})")
    });
    let mut args = vec!["--multi"];
    args.extend_from_slice(COMMON_FZF_LAYOUT_ARGS);
    args.extend([
        "--delimiter",
        "\t",
        "--nth",
        "2",
        "--id-nth",
        "1,2",
        "--track",
        "--pointer",
        "▌",
        "--marker",
        "✔",
        "--ellipsis",
        "...",
        "--header",
        header.as_str(),
        "--prompt",
        prompt.as_str(),
        "--preview-window",
        "down:55%:wrap",
        "--bind",
        "alt-j:last,alt-k:first",
        "--bind",
        reload.as_str(),
    ]);
    if removing {
        args.extend(["--bind", "alt-c:accept"]);
    }
    if let Some(timing_bind) = &timing_bind {
        args.extend(["--bind", timing_bind.as_str()]);
    }
    let preview = format!(
        "{} __preview {mode} \"{{}}\"",
        shell_quote(executable.to_string_lossy().as_ref()),
    );
    args.extend(["--preview", preview.as_str()]);
    let query_value = query.join(" ");
    if !query_value.is_empty() {
        args.extend(["--query", query_value.as_str()]);
    }
    let mut child = Command::new("fzf")
        .args(args)
        .env("FZF_DEFAULT_OPTS", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("cannot start fzf")?;
    let stdin = child.stdin.take().context("cannot open fzf stdin")?;
    let (writer_tx, writer_rx) = std::sync::mpsc::sync_channel(1);
    let writer_started = Instant::now();
    std::thread::spawn(move || {
        let mut stdin = BufWriter::new(stdin);
        let result = write_input(&mut stdin).and_then(|()| stdin.flush().map_err(Into::into));
        timing_event("writer_done", writer_started);
        let _ = writer_tx.send(result);
    });
    let output = child.wait_with_output()?;
    match writer_rx.try_recv() {
        Ok(Err(error))
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::BrokenPipe) => {}
        Ok(result) => result?,
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            anyhow::bail!("fzf input writer terminated unexpectedly");
        }
    }
    if let (Some(marker), Some(started)) = (marker, started) {
        if let Ok(value) = fs::read_to_string(&marker)
            && let Ok(ready_ns) = value.parse::<u128>()
        {
            let started_ns = started.duration_since(UNIX_EPOCH)?.as_nanos();
            if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
                eprintln!(
                    "picker_timing picker={mode} phase=interactive elapsed_ms={}",
                    ready_ns.saturating_sub(started_ns) / 1_000_000
                );
            }
        }
        let _ = fs::remove_file(marker);
    }
    if !classify_picker_status(output.status, mode)? {
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .context("fzf returned invalid UTF-8")
        .map(Some)
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
