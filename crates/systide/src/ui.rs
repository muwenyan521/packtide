use crate::messages::{Lang, msg};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, ExitStatus};
use std::time::Instant;
use system_tools_core::{
    BackendId, BuiltinBackend, CapabilitySet, ExecutableResolver, NativeBackend, PackageBackend,
    current_executable, detect_native_backend_from_file, require_command_for, run_capture_path,
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

pub(crate) fn collect_update_rows(lang: Lang) -> Result<String> {
    let native = detect_native_backend_from_file("/etc/os-release").ok();
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    collect_update_rows_with_resolver(lang, native, &resolver)
}

fn collect_update_rows_with_resolver(
    lang: Lang,
    native: Option<NativeBackend>,
    resolver: &ExecutableResolver,
) -> Result<String> {
    let mut rows = String::new();
    let mut seen = HashSet::new();
    let arch_updates = native.is_some_and(|backend| {
        matches!(backend, NativeBackend::Pacman)
            && BuiltinBackend::new(BackendId::Pacman)
                .capabilities()
                .contains(CapabilitySet::UPDATES)
    });
    let checkupdates = arch_updates
        .then(|| resolver.resolve(OsStr::new("checkupdates")))
        .flatten();
    let helper = arch_updates
        .then(|| {
            resolver
                .resolve(OsStr::new("paru"))
                .or_else(|| resolver.resolve(OsStr::new("yay")))
        })
        .flatten();
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
        append_source_rows(
            &mut rows,
            &mut seen,
            crate::messages::source_label(lang, "pacman"),
            "34",
            &output,
        );
    }
    if let Some(output) = aur {
        append_source_rows(
            &mut rows,
            &mut seen,
            crate::messages::source_label(lang, "aur"),
            "35",
            &output,
        );
    }
    if let Some(output) = flatpak {
        append_source_rows(
            &mut rows,
            &mut seen,
            crate::messages::source_label(lang, "flatpak"),
            "36",
            &output,
        );
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

pub(crate) fn show_update_list(lang: Lang) -> Result<Option<bool>> {
    let picker_started = Instant::now();
    require_command_for("fzf", "the system update list picker", false).map_err(|_| {
        anyhow::anyhow!(
            "{} ({})",
            msg(lang, "backend.missing_tool"),
            msg(lang, "capability.catalog")
        )
    })?;
    let rows = collect_update_rows(lang)?;
    if rows.is_empty() {
        println!("{}", msg(lang, "list.empty"));
        return Ok(None);
    }
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
        "ctrl-r:change-prompt({})+reload-sync({reload})",
        msg(lang, "list.refresh"),
    );
    let reset_prompt = format!("load:change-prompt({})", msg(lang, "list.prompt"));
    let mut args = COMMON_FZF_LAYOUT_ARGS.to_vec();
    args.extend([
        "--delimiter=\t",
        "--nth=2",
        "--id-nth=2",
        "--track",
        "--prompt",
        msg(lang, "list.prompt"),
        "--header",
        msg(lang, "list.header"),
        "--bind",
        bind.as_str(),
        "--bind",
        reset_prompt.as_str(),
    ]);
    let mut child = Command::new("fzf")
        .args(args)
        .env("FZF_DEFAULT_OPTS", "")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .with_context(|| msg(lang, "list.start_failed"))?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(rows.as_bytes())?;
    }
    let status = child.wait()?;
    classify_fzf_status(status).map(Some).map_err(|error| {
        anyhow::anyhow!(
            "{}",
            msg(lang, "list.failed").replace("{error}", &error.to_string())
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{classify_fzf_status, collect_update_rows_with_resolver};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;
    use std::path::PathBuf;
    use std::process::ExitStatus;
    use system_tools_core::{ExecutableResolver, NativeBackend};

    fn fixture() -> (PathBuf, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "systide-ui-native-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).expect("create fixture");
        let marker_dir = path.join("markers");
        fs::create_dir(&marker_dir).expect("create marker directory");
        for (name, body) in [
            ("checkupdates", "printf 'repo 1 -> 2\\n'"),
            ("paru", "printf 'aur 1 -> 2\\n'"),
            ("flatpak", "printf 'org.example.App 2\\n'"),
        ] {
            let file = path.join(name);
            let marker = marker_dir.join(name);
            fs::write(
                &file,
                format!("#!/bin/sh\ntouch '{}'\n{}\n", marker.display(), body),
            )
            .expect("write fixture command");
            let mut permissions = fs::metadata(&file)
                .expect("stat fixture command")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&file, permissions).expect("chmod fixture command");
        }
        (path, marker_dir)
    }

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
    fn non_arch_native_list_skips_arch_only_providers() {
        let (path, markers) = fixture();
        let resolver = ExecutableResolver::from_path(Some(path.as_os_str()));
        let rows =
            collect_update_rows_with_resolver(super::Lang::En, Some(NativeBackend::Apt), &resolver)
                .expect("collect non-Arch rows");
        assert!(!rows.contains("[Pacman]"));
        assert!(!rows.contains("[AUR]"));
        assert!(rows.contains("[Flatpak]"));
        assert!(!markers.join("checkupdates").exists());
        assert!(!markers.join("paru").exists());
        assert!(markers.join("flatpak").exists());
        fs::remove_dir_all(path).expect("remove fixture");
    }

    #[test]
    fn arch_native_list_preserves_pacman_and_aur_providers() {
        let (path, markers) = fixture();
        let resolver = ExecutableResolver::from_path(Some(path.as_os_str()));
        let rows = collect_update_rows_with_resolver(
            super::Lang::En,
            Some(NativeBackend::Pacman),
            &resolver,
        )
        .expect("collect Arch rows");
        assert!(rows.contains("[Pacman]"));
        assert!(rows.contains("[AUR]"));
        assert!(rows.contains("[Flatpak]"));
        assert!(markers.join("checkupdates").exists());
        assert!(markers.join("paru").exists());
        assert!(markers.join("flatpak").exists());
        fs::remove_dir_all(path).expect("remove fixture");
    }
}
