use anyhow::{Result, bail};
use system_tools_core::{PackageSource, run_capture};

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
    let operation = match (kind, source) {
        ("remove", Some(PackageSource::Flatpak)) => vec!["info", package],
        ("remove", Some(PackageSource::Pacman | PackageSource::Aur)) | ("downgrade", Some(_)) => {
            vec!["-Qi", package]
        }
        ("install", Some(_)) => vec!["-Si", package],
        _ => bail!("unknown preview kind: {kind}"),
    };
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
    println!(
        "\x1b[1;36m{package}\x1b[0m  \x1b[2m{source_label} · {version}\x1b[0m\n\x1b[2m────────────────────────────────────────\x1b[0m"
    );
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

fn colorize_metadata(output: &str) -> String {
    use std::fmt::Write as _;

    let mut rendered = String::with_capacity(output.len() + output.lines().count() * 10);
    for (index, line) in output.lines().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        if let Some((label, separator, value)) = split_metadata_line(line) {
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
    use super::colorize_metadata;

    #[test]
    fn colors_pacman_metadata_labels_without_changing_values() {
        let rendered = colorize_metadata("Name : bash\nArchitecture : x86_64\n");
        assert!(rendered.contains("\x1b[1;36mName\x1b[0m : bash"));
        assert!(rendered.contains("\x1b[1;36mArchitecture\x1b[0m : x86_64"));
        assert!(colorize_metadata("Name: bash\n").contains("\x1b[1;36mName\x1b[0m: bash"));
        assert!(colorize_metadata("描述：shell\n").contains("\x1b[1;36m描述\x1b[0m：shell"));
    }
}
