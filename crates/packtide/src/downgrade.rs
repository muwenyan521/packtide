use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use system_tools_core::PackageSource;
use system_tools_core::{
    CommandPlan, CommandPrivilege, current_executable, run_capture, run_command_plan,
};
use unicode_width::UnicodeWidthStr;

use crate::transaction::print_summary;
use crate::ui::{COMMON_FZF_LAYOUT_ARGS, classify_picker_status, picker_columns, picker_header};

pub fn run(query: &[String]) -> Result<()> {
    let mut downgrade_path = None;
    for (command, capability) in [
        ("fzf", "the package downgrade picker"),
        ("pacman", "downgrade package information"),
        ("downgrade", "the package downgrade transaction"),
    ] {
        let capability_key = match command {
            "fzf" => "capability.catalog",
            "pacman" => "capability.details",
            "downgrade" => "capability.updates",
            _ => "capability.details",
        };
        let path = crate::app::require_command_for(command, capability_key, capability, false)?;
        if command == "downgrade" {
            downgrade_path = Some(path);
        }
    }
    if !["paru", "yay"]
        .iter()
        .any(|command| system_tools_core::command_exists(command))
    {
        crate::app::package_helper_for("downgrade package details")?;
    }
    let installed = rows()?;
    let helper = if system_tools_core::command_exists("paru") {
        "paru"
    } else {
        "yay"
    };
    let lang = crate::locale::current();
    let title = crate::locale::text(lang, "downgrade.title", &[]);
    let actions = crate::locale::text(lang, "downgrade.actions", &[]);
    let prompt = crate::locale::text(lang, "downgrade.prompt", &[]);
    let using = crate::locale::text(lang, "picker.using", &[("helper", helper)]);
    let header = picker_header(
        title.as_str(),
        using.as_str(),
        actions.as_str(),
        picker_columns(),
        "1;33",
    );
    let mut args = vec!["--multi"];
    args.extend_from_slice(COMMON_FZF_LAYOUT_ARGS);
    args.extend([
        "--header",
        header.as_str(),
        "--prompt",
        prompt.as_str(),
        "--nth",
        "2",
        "--id-nth",
        "2",
        "--track",
        "--preview-window",
        "down:55%:wrap",
        "--info=inline",
    ]);
    let preview = format!(
        "sh -c 'exec {} __preview downgrade \"$1\"' packtide-preview \"{{}}\"",
        crate::ui::preview::shell_quote(current_executable()?.to_string_lossy().as_ref())
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
    child
        .stdin
        .take()
        .context("cannot open fzf stdin")?
        .write_all(installed.as_bytes())?;
    let output = child.wait_with_output()?;
    if !classify_picker_status(output.status, "downgrade")? {
        println!("{}", crate::locale::text(lang, "downgrade.none", &[]));
        return Ok(());
    }
    let selected = String::from_utf8(output.stdout).context("fzf returned invalid UTF-8")?;
    let packages = selected_packages(&selected);
    if packages.is_empty() {
        println!("{}", crate::locale::text(lang, "downgrade.none", &[]));
        return Ok(());
    }
    println!(
        "\x1b[33m{}\x1b[0m",
        crate::locale::text(
            lang,
            "downgrade.prepare",
            &[("packages", &packages.join(" "))]
        )
    );
    println!(
        "\x1b[1;31m{}\x1b[0m",
        crate::locale::text(lang, "downgrade.warning", &[])
    );
    let command = transaction_args(&packages);
    let targets = packages.iter().map(String::as_str).collect::<Vec<_>>();
    print_summary("downgrade", "downgrade", "sudo", &targets);
    run_downgrade_transaction(
        downgrade_path
            .as_deref()
            .expect("downgrade preflight resolved a transaction executable"),
        &command,
    )?;
    Ok(())
}

fn run_downgrade_transaction(program: &Path, command: &[String]) -> Result<()> {
    #[cfg(debug_assertions)]
    if let Some(program) = std::env::var_os("PACKTIDE_TEST_DOWNGRADE_BIN") {
        let status = Command::new(program).args(command).status()?;
        if !status.success() {
            anyhow::bail!("downgrade test transaction exited with {status}");
        }
        return Ok(());
    }
    let mut plan = CommandPlan::new(program.to_owned())
        .with_env_remove("LD_PRELOAD")
        .with_env_remove("LD_LIBRARY_PATH")
        .with_locale("C")
        .with_privilege(CommandPrivilege::Elevated);
    plan.args
        .extend(command.iter().skip(1).map(std::ffi::OsString::from));
    run_command_plan(&plan)?;
    Ok(())
}

fn selected_packages(selected: &str) -> Vec<String> {
    selected
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(str::to_owned)
        .collect()
}

fn transaction_args(packages: &[String]) -> Vec<String> {
    std::iter::once("downgrade".to_owned())
        .chain(packages.iter().cloned())
        .collect()
}

fn rows() -> Result<String> {
    let sync = run_capture("pacman", &["--color=never", "-Sl"], true)?.stdout;
    let installed = run_capture("pacman", &["--color=never", "-Q"], false)?.stdout;
    let mut repos = std::collections::HashMap::new();
    for line in sync.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(repo), Some(name)) = (parts.next(), parts.next()) {
            repos.entry(name.to_owned()).or_insert(repo.to_owned());
        }
    }
    let mut rows = String::new();
    for line in installed.lines() {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let version = parts.next().unwrap_or("-");
        let source = repos.get(name).map(String::as_str).unwrap_or("aur");
        write_row(&mut rows, source, name, version);
        rows.push('\n');
    }
    Ok(rows)
}

#[cfg(test)]
fn render_row(source: &str, name: &str, version: &str) -> String {
    let mut row = String::with_capacity(source.len() + name.len() + version.len() + 18);
    write_row(&mut row, source, name, version);
    row
}

fn write_row(row: &mut String, source: &str, name: &str, version: &str) {
    const SPACES: &str = "                                                                ";
    let source_padding = 16usize.saturating_sub(UnicodeWidthStr::width(source));
    let name_padding = 30usize.saturating_sub(UnicodeWidthStr::width(name));
    let source_color = if source == "aur" {
        crate::ui::source_color(PackageSource::Aur)
    } else {
        crate::ui::source_color(PackageSource::Pacman)
    };
    row.push_str("\x1b[");
    row.push_str(source_color);
    row.push('m');
    row.push_str(source);
    row.push_str(&SPACES[..source_padding.min(SPACES.len())]);
    row.push_str("\x1b[0m ");
    row.push_str("\x1b[1m");
    row.push_str(name);
    row.push_str("\x1b[0m");
    row.push_str(&SPACES[..name_padding.min(SPACES.len())]);
    row.push(' ');
    row.push_str("\x1b[2m");
    row.push_str(version);
    row.push_str("\x1b[0m");
}

#[cfg(test)]
mod tests {
    use super::{render_row, selected_packages, transaction_args};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn downgrade_columns_use_terminal_width_and_do_not_clip_long_versions() {
        let row = render_row("仓库", "示例包", "版本-超长-版本-1");
        assert!(row.contains("\x1b[34m"));
        let row = crate::ui::strip_ansi(&row);
        let mut columns = row.split_whitespace();
        assert_eq!(columns.next(), Some("仓库"));
        assert_eq!(columns.next(), Some("示例包"));
        let version = columns.next().expect("version column");
        assert_eq!(version, "版本-超长-版本-1");
        assert!(UnicodeWidthStr::width(version) > 0);
        assert!(row.contains("示例包"));
    }

    #[test]
    fn accepted_rows_keep_names_and_build_exact_transaction_argv() {
        let selected = "core  bash 5.3-1\nextra  gcc 15.1-1\n";
        let packages = selected_packages(selected);
        assert_eq!(packages, ["bash", "gcc"]);
        assert_eq!(transaction_args(&packages), ["downgrade", "bash", "gcc"]);
    }
}
