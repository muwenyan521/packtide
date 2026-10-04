use anyhow::{Result, bail};
use system_tools_core::{
    BackendId, BackendRegistry, NativePackageKey, PackageBackend, PackageId, PackageIdentity,
    PackageSource, ReadOperation,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::source_metadata::{PackageRoute, package_route};
use crate::sources::valid_package_name;

use super::{parse_package_identity, parse_package_row};

pub(crate) fn preview_command(args: &[String]) -> Result<()> {
    let kind = args.first().map(String::as_str).unwrap_or_default();
    let raw_row = args.get(1).map(String::as_str).unwrap_or_default();
    let parsed = parse_package_row(raw_row);
    let display_package = parsed
        .as_ref()
        .map(|row| row.name.as_str())
        .unwrap_or_default();
    let lang = crate::locale::current();
    if display_package.is_empty() {
        bail!(
            "{}",
            crate::locale::text(lang, "preview.missing_package", &[])
        );
    }
    if !valid_package_name(display_package) {
        bail!("{}", crate::locale::text(lang, "preview.invalid_row", &[]));
    }
    let source = parsed.as_ref().map(|row| row.source);
    let parsed_identity = parse_package_identity(raw_row)
        .ok_or_else(|| anyhow::anyhow!("invalid package identity in preview row"))?;
    let backend_id = if package_route(parsed_identity.backend) == PackageRoute::ArchRepository
        && matches!(kind, "install" | "downgrade")
    {
        crate::source_metadata::aur_backend(crate::app::package_helper().unwrap_or("paru"))
    } else {
        parsed_identity.backend
    };
    let identity_key = if matches!(kind, "remove" | "downgrade")
        && parsed_identity.backend != BackendId::Flatpak
    {
        format!("detail-qi:{display_package}")
    } else if matches!(kind, "install" | "downgrade")
        && let Some(repository) = parsed.as_ref().and_then(|row| row.repository.as_deref())
    {
        format!("{repository}/{display_package}")
    } else {
        parsed_identity.native_key.as_str().to_owned()
    };
    let identity_kind = if backend_id == parsed_identity.backend {
        parsed_identity.kind
    } else {
        backend_id.default_kind()
    };
    let identity_scope = if backend_id == parsed_identity.backend {
        parsed_identity.scope
    } else {
        backend_id.default_scope()
    };
    let registry = BackendRegistry::default();
    let backend = registry
        .backend(backend_id)
        .ok_or_else(|| anyhow::anyhow!("unknown backend {}", backend_id.as_str()))?;
    let mut identity = PackageIdentity::new(
        backend_id,
        identity_kind,
        identity_scope,
        NativePackageKey::new(identity_key)?,
    );
    if let Some(origin) = &parsed_identity.origin {
        identity = identity.with_origin(origin.clone());
    }
    if let Some(display_name) = &parsed_identity.display_name {
        identity = identity.with_display_name(display_name.clone());
    }
    let supported = matches!(kind, "remove" | "install") && source.is_some()
        || kind == "downgrade" && source.is_some_and(crate::source_metadata::supports_downgrade);
    if !supported {
        bail!(
            "{}",
            crate::locale::text(lang, "preview.unknown_kind", &[("kind", kind)])
        );
    }
    let detail = backend.read(ReadOperation::Details {
        package: PackageId::new(identity.key().as_str())?,
        scope: identity.scope,
    });
    let (metadata, failure) = match detail.as_ref() {
        Ok(result) => match result.details.as_ref() {
            Some(details) => {
                let failure = (!details.success).then(|| {
                    let message = details.stderr.trim();
                    if message.is_empty() {
                        format!("{} exited with {}", backend_id.as_str(), details.status)
                    } else {
                        message.to_owned()
                    }
                });
                (details.stdout.as_str(), failure)
            }
            None => ("", None),
        },
        Err(error) => ("", Some(error.to_string())),
    };
    let source_label = crate::locale::source_label(lang, source.unwrap_or(PackageSource::Pacman));
    let version = preview_version(raw_row);
    let width = std::env::var("FZF_PREVIEW_COLUMNS")
        .or_else(|_| std::env::var("COLUMNS"))
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .unwrap_or(80);
    println!(
        "{}",
        preview_header(display_package, &source_label, &version, width)
    );
    if metadata.trim().is_empty() {
        println!("{}", crate::locale::text(lang, "preview.empty", &[]));
    } else {
        print!("{}", colorize_metadata(metadata));
    }
    if let Some(error) = failure {
        println!(
            "\x1b[1;31m!\x1b[0m {}",
            crate::locale::text(lang, "preview.failed", &[("error", &error)])
        );
    }
    Ok(())
}

fn strip_outer_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn preview_version(raw_row: &str) -> String {
    let stripped = super::strip_ansi(raw_row);
    let clean_row = strip_outer_quotes(&stripped);
    clean_row
        .split('\t')
        .nth(2)
        .and_then(|value| value.split_whitespace().next())
        .filter(|value| !value.is_empty() && *value != "-")
        .unwrap_or("unknown")
        .to_owned()
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
    use super::{colorize_metadata, preview_header, preview_version};
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
    fn preserves_unstructured_colored_section_headers() {
        let rendered = colorize_metadata("\x1b[1;35mOptional Dependencies\x1b[0m\n");
        assert!(rendered.contains("\x1b[1;35mOptional Dependencies\x1b[0m"));
        assert!(!rendered.contains("\x1b[1;36mOptional Dependencies"));
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

    #[test]
    fn preview_version_ignores_shell_quotes_ansi_and_install_badge() {
        let row = "'\x1b[34mcore            \x1b[0m\tbash                               \t\x1b[2m5.3-1\x1b[0m                \x1b[32m✔ [Installed]\x1b[0m'";
        assert_eq!(preview_version(row), "5.3-1");
    }
}
