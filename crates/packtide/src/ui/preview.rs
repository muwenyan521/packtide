use anyhow::{Result, bail};
use system_tools_core::{PackageSource, run_capture};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::sources::valid_package_name;

use super::parse_package_row;

pub(crate) fn preview_command(args: &[String]) -> Result<()> {
    let kind = args.first().map(String::as_str).unwrap_or_default();
    let raw_row = args.get(1).map(String::as_str).unwrap_or_default();
    let parsed = parse_package_row(raw_row);
    let package = parsed
        .as_ref()
        .map(|row| row.name.as_str())
        .unwrap_or_default();
    if package.is_empty() {
        bail!("preview requires a package name");
    }
    if !valid_package_name(package) {
        bail!("preview received an invalid package row");
    }
    let source = parsed.as_ref().map(|row| row.source);
    let helper = crate::app::package_helper().unwrap_or("paru");
    let program = match (kind, source) {
        ("remove", Some(PackageSource::Flatpak)) => "flatpak",
        ("remove", Some(PackageSource::Pacman | PackageSource::Aur))
        | ("downgrade", Some(_))
        | ("install", Some(_)) => helper,
        _ => bail!("unknown preview kind: {kind}"),
    };
    let mut operation = match (kind, source) {
        ("remove", Some(PackageSource::Flatpak)) => vec!["info", package],
        ("remove", Some(PackageSource::Pacman | PackageSource::Aur)) | ("downgrade", Some(_)) => {
            vec!["-Qi", package]
        }
        ("install", Some(_)) => vec!["-Si", package],
        _ => bail!("unknown preview kind: {kind}"),
    };
    if source != Some(PackageSource::Flatpak) {
        operation.insert(0, "--color=always");
    }
    let output = run_capture(program, &operation, true)?;
    let lang = crate::locale::current();
    let source_key = match source {
        Some(PackageSource::Pacman) => "source.pacman",
        Some(PackageSource::Aur) => "source.aur",
        Some(PackageSource::Flatpak) => "source.flatpak",
        None => "source.pacman",
    };
    let source_label = crate::locale::text(lang, source_key, &[]);
    let version = parsed
        .as_ref()
        .and_then(|_| raw_row.split('\t').nth(2))
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "-")
        .unwrap_or("unknown");
    let width = std::env::var("FZF_PREVIEW_COLUMNS")
        .or_else(|_| std::env::var("COLUMNS"))
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .unwrap_or(80);
    println!("{}", preview_header(package, &source_label, version, width));
    if output.stdout.trim().is_empty() {
        println!("{}", crate::locale::text(lang, "preview.empty", &[]));
    } else {
        print!("{}", colorize_metadata(&output.stdout));
    }
    if !output.status.success() {
        let message = output.stderr.trim();
        let error = if message.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            message.to_owned()
        };
        println!(
            "\x1b[1;31m!\x1b[0m {}",
            crate::locale::text(lang, "preview.failed", &[("error", &error)])
        );
    }
    Ok(())
}

fn preview_header(package: &str, source: &str, version: &str, width: usize) -> String {
    let details = format!("{source} · {version}");
    let name_width = UnicodeWidthStr::width(package);
    let details_width = UnicodeWidthStr::width(details.as_str());
    let (name, details) = if name_width + details_width + 2 <= width {
        (package.to_owned(), details)
    } else {
        let name_budget = name_width.min(width.saturating_sub(2) / 2);
        let details_budget = width.saturating_sub(name_budget + 2);
        (
            truncate_display(package, name_budget),
            truncate_display(&details, details_budget),
        )
    };
    let separator = "─".repeat(width.min(40));
    format!("\x1b[1;36m{name}\x1b[0m  \x1b[2m{details}\x1b[0m\n\x1b[2m{separator}\x1b[0m")
}

fn truncate_display(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_owned();
    }
    let content_width = width.saturating_sub(3);
    let mut result = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + char_width > content_width {
            break;
        }
        result.push(ch);
        used += char_width;
    }
    result.push_str(&".".repeat(width.saturating_sub(used)));
    result
}

fn colorize_metadata(output: &str) -> String {
    use std::fmt::Write as _;

    let mut rendered = String::with_capacity(output.len() + output.lines().count() * 10);
    for (index, line) in output.lines().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        if let Some((label, separator, value)) = split_metadata_line(line) {
            let label = super::strip_ansi(label);
            write!(rendered, "\x1b[1;36m{label}\x1b[0m{separator}{value}")
                .expect("writing preview metadata to String cannot fail");
        } else {
            rendered.push_str(line);
        }
    }
    if output.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

fn split_metadata_line(line: &str) -> Option<(&str, &str, &str)> {
    if let Some((label, value)) = line.split_once(" : ") {
        return Some((label.trim_end(), " : ", value));
    }
    if let Some((label, value)) = line.split_once(": ") {
        return Some((label.trim_end(), ": ", value));
    }
    line.split_once('：')
        .map(|(label, value)| (label.trim_end(), "：", value.trim_start()))
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::{colorize_metadata, preview_header};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn colors_pacman_metadata_labels_without_changing_values() {
        let rendered = colorize_metadata("Name : bash\nArchitecture : x86_64\n");
        assert!(rendered.contains("\x1b[1;36mName\x1b[0m : bash"));
        assert!(rendered.contains("\x1b[1;36mArchitecture\x1b[0m : x86_64"));
        assert!(colorize_metadata("Name: bash\n").contains("\x1b[1;36mName\x1b[0m: bash"));
        assert!(colorize_metadata("描述：shell\n").contains("\x1b[1;36m描述\x1b[0m：shell"));
    }

    #[test]
    fn strips_ansi_from_metadata_labels_but_preserves_colored_values() {
        let rendered = colorize_metadata("\x1b[1mName\x1b[0m : \x1b[32mbash\x1b[0m\n");
        assert!(rendered.contains("\x1b[1;36mName\x1b[0m : \x1b[32mbash\x1b[0m"));
        assert!(!rendered.contains("\x1b[1;36m\x1b[1mName"));
    }

    #[test]
    fn preview_header_fits_narrow_windows_without_changing_package_identity() {
        let header = preview_header("示例软件包-very-long-name", "官方源", "2026.09.30-long", 24);
        let lines = header
            .lines()
            .map(crate::ui::strip_ansi)
            .collect::<Vec<_>>();
        assert!(
            lines
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= 24)
        );
        assert!(lines[0].contains("..."));
    }
}
