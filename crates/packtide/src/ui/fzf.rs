use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{BufWriter, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
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
    select_rows_with_input(helper, removing, query, started, None, move |stdin| {
        stdin.write_all(rows.as_bytes())?;
        Ok(())
    })
}

#[allow(dead_code)]
pub(crate) fn select_install_catalog_streaming(
    helper: &str,
    pacman: &std::path::Path,
    refresh: bool,
    query: &[String],
    started: Option<SystemTime>,
) -> Result<Option<String>> {
    let pacman = pacman.to_path_buf();
    let query = query.to_vec();
    let picker_query = query.clone();
    let has_rows = Arc::new(AtomicBool::new(false));
    let writer_has_rows = Arc::clone(&has_rows);
    select_rows_with_input(
        helper,
        false,
        &picker_query,
        started,
        Some(has_rows),
        move |stdin| {
            let source_started = Instant::now();
            let mut official_first = false;
            let mut rows_started = false;
            let catalog = crate::sources::install_rows_streaming(
                &pacman,
                refresh,
                &query,
                false,
                |record| {
                    if !official_first {
                        timing_event("official_first", source_started);
                        official_first = true;
                    }
                    writer_has_rows.store(true, Ordering::Release);
                    if rows_started {
                        stdin.write_all(b"\n")?;
                    }
                    super::rows::write_package_row(
                        stdin,
                        record,
                        super::rows::PackageListMode::Install,
                    )
                    .context("failed writing package row to fzf")?;
                    rows_started = true;
                    Ok(())
                },
            )?;
            timing_event("official_done", source_started);
            timing_event("aur_start", source_started);
            super::rows::write_aur_install_rows(&catalog, stdin, rows_started)
                .context("failed writing AUR rows to fzf")?;
            if catalog
                .aur_names
                .lines()
                .any(crate::sources::valid_package_name)
            {
                writer_has_rows.store(true, Ordering::Release);
            }
            let optional = crate::sources::optional_install_rows(&query, refresh)?;
            if !optional.is_empty() {
                if rows_started {
                    stdin.write_all(b"\n")?;
                }
                for (index, record) in optional.iter().enumerate() {
                    if index > 0 {
                        stdin.write_all(b"\n")?;
                    }
                    super::rows::write_package_row(
                        stdin,
                        record,
                        super::rows::PackageListMode::Install,
                    )
                    .context("failed writing optional package row to fzf")?;
                }
                writer_has_rows.store(true, Ordering::Release);
            }
            timing_event("aur_done", source_started);
            Ok(())
        },
    )
}

fn timing_event(phase: &str, started: Instant) {
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "picker_timing picker=install phase={phase} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn query_reload_bind(executable: &std::path::Path, refresh: bool) -> String {
    format!(
        "change:reload({})",
        query_reload_command(executable, refresh)
    )
}

fn query_reload_command(executable: &std::path::Path, refresh: bool) -> String {
    let executable = shell_quote(executable.to_string_lossy().as_ref());
    let script = shell_quote(include_str!("query_reload.sh"));
    let refresh = if refresh { "refresh" } else { "cached" };
    format!("exec bash -c {script} {executable} {{q}} {refresh}")
}

fn push_wrapped_item(output: &mut String, line_width: &mut usize, item: &str, columns: usize) {
    let item_width = UnicodeWidthStr::width(item);
    let separator_width = if *line_width == 0 { 0 } else { 3 };
    if *line_width > 0 && line_width.saturating_add(separator_width + item_width) > columns {
        output.push('\n');
        *line_width = 0;
    }
    if *line_width > 0 {
        output.push_str(" | ");
        *line_width += separator_width;
    }
    output.push_str(item);
    *line_width += item_width;
}

pub(crate) fn wrap_shortcut_line(shortcuts: &str, columns: usize) -> String {
    let mut output = String::with_capacity(shortcuts.len());
    let mut line_width = 0;
    for item in shortcuts.split(" | ") {
        push_wrapped_item(&mut output, &mut line_width, item, columns);
    }
    output
}

pub(crate) fn picker_columns() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(80)
        .saturating_sub(8)
        .max(1)
}

pub(crate) fn picker_header(
    title: &str,
    using: &str,
    shortcuts: &str,
    columns: usize,
    title_color: &str,
) -> String {
    let title_width = UnicodeWidthStr::width(title);
    let using_width = UnicodeWidthStr::width(using);
    let title_separator = if title_width.saturating_add(using_width + 2) < columns {
        " ".repeat(columns - title_width - using_width)
    } else {
        "\n".to_owned()
    };
    let shortcuts = wrap_shortcut_line(shortcuts, columns);
    format!(
        "\x1b[{title_color}m{title}\x1b[0m{title_separator}\x1b[33m{using}\x1b[0m\n\x1b[2m{shortcuts}\x1b[0m"
    )
}

fn select_rows_with_input(
    helper: &str,
    removing: bool,
    query: &[String],
    started: Option<SystemTime>,
    rows_available: Option<Arc<AtomicBool>>,
    write_input: impl FnOnce(&mut dyn Write) -> Result<()> + Send + 'static,
) -> Result<Option<String>> {
    let executable = current_executable()?;
    let mode = if removing { "remove" } else { "install" };
    let lang = crate::locale::current();
    let reload_command = if removing {
        format!(
            "PACKTIDE_REMOVE_LIST_ONLY=1 {} remove",
            shell_quote(executable.to_string_lossy().as_ref()),
        )
    } else {
        query_reload_command(&executable, true)
    };
    let reload = format!(
        "ctrl-r:change-prompt({})+reload({reload_command})",
        crate::locale::text(
            lang,
            if removing {
                "remove.refresh"
            } else {
                "install.refresh"
            },
            &[],
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
    let columns = picker_columns();
    let title_color = if removing { "1;33" } else { "1;36" };
    let shortcuts = format!("{action} | {refresh} | {exit}");
    let header = picker_header(
        title.as_str(),
        using.as_str(),
        shortcuts.as_str(),
        columns,
        title_color,
    );
    let marker = if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Some(std::env::temp_dir().join(format!("packtide-ready-{}-{nonce}", std::process::id())))
    } else {
        None
    };
    let timing_action = marker.as_ref().map(|marker| {
        let mark_command = format!(
            "{} __timing-mark {}",
            shell_quote(executable.to_string_lossy().as_ref()),
            shell_quote(marker.to_string_lossy().as_ref())
        );
        format!("execute-silent({mark_command})")
    });
    let load_bind = match timing_action {
        Some(timing_action) => {
            format!("load:change-prompt({prompt})+{timing_action}")
        }
        None => format!("load:change-prompt({prompt})"),
    };
    let query_state = (!removing).then(|| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "packtide-query-state-{}-{nonce}",
            std::process::id()
        ))
    });
    if let Some(state) = &query_state {
        fs::write(state, "0").context("cannot initialize query generation state")?;
    }
    let mut args = vec!["--multi"];
    args.extend_from_slice(COMMON_FZF_LAYOUT_ARGS);
    args.extend([
        "--delimiter",
        "\t",
        "--nth",
        "2",
        "--with-nth",
        "2..",
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
        "--bind",
        load_bind.as_str(),
    ]);
    if removing {
        args.extend(["--bind", "alt-c:accept"]);
    }
    let preview = format!(
        "bash -c 'exec {} __preview {mode} \"$1\"' packtide-preview \"{{}}\"",
        shell_quote(executable.to_string_lossy().as_ref()),
    );
    args.extend(["--preview", preview.as_str()]);
    let query_value = query
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !query_value.is_empty() {
        args.extend(["--query", query_value.as_str()]);
    }
    let mut fzf_command = Command::new("fzf");
    fzf_command.args(args).env("FZF_DEFAULT_OPTS", "");
    if let Some(state) = &query_state {
        fzf_command.env("PACKTIDE_QUERY_STATE", state);
    }
    let mut child = fzf_command
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
    if let Some(state) = query_state {
        let _ = fs::remove_file(&state);
    }
    match writer_rx.recv_timeout(std::time::Duration::from_millis(50)) {
        Ok(Err(error))
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::BrokenPipe) => {}
        Ok(result) => result?,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
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
        if rows_available.is_some_and(|rows| !rows.load(Ordering::Acquire)) {
            return Ok(Some(String::new()));
        }
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .context("fzf returned invalid UTF-8")
        .map(Some)
}

#[cfg(test)]
#[path = "fzf_query_tests.rs"]
mod query_tests;

#[cfg(test)]
mod tests {
    use super::{classify_fzf_status, picker_header, query_reload_bind, wrap_shortcut_line};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use unicode_width::UnicodeWidthStr;

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

    #[test]
    fn wraps_shortcuts_without_dropping_any_items() {
        let shortcuts =
            wrap_shortcut_line("Tab:select | Enter:install | Ctrl+R:refresh | Esc:exit", 40);

        assert_eq!(
            shortcuts,
            "Tab:select | Enter:install\nCtrl+R:refresh | Esc:exit"
        );
        assert!(
            shortcuts
                .lines()
                .all(|line| UnicodeWidthStr::width(line) <= 40)
        );
    }

    #[test]
    fn wraps_title_and_helper_without_losing_shortcuts() {
        let header = picker_header(
            "PACKTIDE · Install Packages",
            "Using paru",
            "Tab:select | Enter:install | Ctrl+R:refresh | Esc:exit",
            30,
            "1;36",
        );

        assert!(header.contains("Install Packages\x1b[0m\n\x1b[33mUsing paru"));
        assert!(header.contains("Tab:select"));
        assert!(header.contains("Enter:install"));
        assert!(header.contains("Ctrl+R:refresh"));
        assert!(header.contains("Esc:exit"));
    }

    #[test]
    fn query_reload_binding_trims_debounces_and_restores_initial_rows() {
        let bind = query_reload_bind(std::path::Path::new("/tmp/packtide"), true);
        assert!(bind.starts_with("change:reload(exec bash -c 'set -eu"));
        assert!(bind.contains("sleep 0.3"));
        assert!(bind.contains("s/^[[:space:]]*//"));
        assert!(bind.contains("s/[[:space:]]*$//"));
        assert!(bind.contains("${#q}\" -ge 2"));
        assert!(bind.contains("PACKTIDE_INSTALL_LIST_ONLY=1"));
        assert!(bind.contains("PACKTIDE_QUERY_STATE"));
        assert!(bind.contains("kill -TERM -- \"-$$\""));
        assert!(bind.contains("while kill -0 \"$child\""));
        assert!(bind.contains("[ \"$current\" != \"$generation\" ]"));
        assert!(bind.contains("current=$(cat \"$state\""));
        assert!(bind.contains("mktemp"));
        assert!(bind.contains("--refresh"));
        assert!(bind.contains("{q}"));
    }
}
