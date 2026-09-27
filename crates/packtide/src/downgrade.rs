use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};
use system_tools_core::{current_executable, require_command_for, run_capture, run_privileged};
use unicode_width::UnicodeWidthStr;

use crate::transaction::print_summary;
use crate::ui::{COMMON_FZF_LAYOUT_ARGS, NO_SELECTION, classify_picker_status};

pub fn run(query: &[String]) -> Result<()> {
    for (command, capability) in [
        ("fzf", "the package downgrade picker"),
        ("pacman", "downgrade package information"),
        ("downgrade", "the package downgrade transaction"),
    ] {
        require_command_for(command, capability, false)?;
    }
    if !["paru", "yay"]
        .iter()
        .any(|command| system_tools_core::command_exists(command))
    {
        bail!(
            "required AUR helper ('paru' or 'yay') is unavailable for downgrade package details; install paru or yay and retry"
        );
    }
    let installed = rows()?;
    let helper = if system_tools_core::command_exists("paru") {
        "paru"
    } else {
        "yay"
    };
    let header =
        format!("Tab:多选 | Enter:降级 | Esc:退出 | \x1b[33mUsing downgrade & {helper}\x1b[0m");
    let mut args = vec!["--multi"];
    args.extend_from_slice(COMMON_FZF_LAYOUT_ARGS);
    args.extend([
        "--header",
        header.as_str(),
        "--prompt",
        "待降级项目 > ",
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
        "{} __preview downgrade \"{{}}\"",
        current_executable()?.display()
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
        println!("{NO_SELECTION}");
        return Ok(());
    }
    let selected = String::from_utf8(output.stdout).context("fzf returned invalid UTF-8")?;
    let packages: Vec<_> = selected
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    if packages.is_empty() {
        println!("{NO_SELECTION}");
        return Ok(());
    }
    println!(
        "\x1b[33mPreparing to downgrade:\x1b[0m {}",
        packages.join(" ")
    );
    println!("\x1b[1;31mWARNING:\x1b[0m downgrading core libraries can break the system.");
    print_summary("downgrade", "downgrade", "sudo", &packages);
    let mut command = vec!["downgrade"];
    command.extend(packages);
    run_privileged(&command)?;
    Ok(())
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
    row.push_str(source);
    row.push_str(&SPACES[..source_padding.min(SPACES.len())]);
    row.push(' ');
    row.push_str(name);
    row.push_str(&SPACES[..name_padding.min(SPACES.len())]);
    row.push(' ');
    row.push_str(version);
}

#[cfg(test)]
mod tests {
    use super::render_row;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn downgrade_columns_use_terminal_width_and_do_not_clip_long_versions() {
        let row = render_row("仓库", "示例包", "版本-超长-版本-1");
        let mut columns = row.split_whitespace();
        assert_eq!(columns.next(), Some("仓库"));
        assert_eq!(columns.next(), Some("示例包"));
        let version = columns.next().expect("version column");
        assert_eq!(version, "版本-超长-版本-1");
        assert!(UnicodeWidthStr::width(version) > 0);
        assert!(row.contains("示例包"));
    }
}
