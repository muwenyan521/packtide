use super::strip_ansi;
use crate::model::{PackageListing, PackageRecord};
use std::collections::HashSet;
use std::io::{self, Write};
use system_tools_core::PackageSource;
use unicode_width::UnicodeWidthStr;

const SPACES: &[u8] = b"                                                                ";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PackageListMode {
    Install,
    Remove,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PackageRow {
    pub(crate) source: PackageSource,
    pub(crate) repository: Option<String>,
    pub(crate) name: String,
}

pub(crate) fn render_package_rows(records: &[PackageRecord], mode: PackageListMode) -> String {
    let mut rows = Vec::with_capacity(records.len().saturating_mul(96));
    for (index, record) in records.iter().enumerate() {
        if index > 0 {
            rows.push(b'\n');
        }
        write_package_row(&mut rows, record, mode).expect("writing to a vector cannot fail");
    }
    String::from_utf8(rows).expect("rendered package rows are valid UTF-8")
}

pub(crate) fn write_install_catalog<W: Write>(
    catalog: &crate::sources::InstallCatalog,
    mut output: W,
) -> io::Result<()> {
    let rows_started = write_official_install_rows(catalog, &mut output)?;
    write_aur_install_rows(catalog, &mut output, rows_started)
}

pub(crate) fn write_official_install_rows<W: Write>(
    catalog: &crate::sources::InstallCatalog,
    output: &mut W,
) -> io::Result<bool> {
    let mut rows_started = false;
    for record in &catalog.official {
        if rows_started {
            output.write_all(b"\n")?;
        }
        write_package_row(output, record, PackageListMode::Install)?;
        rows_started = true;
    }
    Ok(rows_started)
}

pub(crate) fn write_aur_install_rows<W: Write + ?Sized>(
    catalog: &crate::sources::InstallCatalog,
    output: &mut W,
    mut rows_started: bool,
) -> io::Result<()> {
    let official = if catalog.official_names.is_empty() {
        catalog
            .official
            .iter()
            .map(|record| record.name.as_str())
            .collect::<HashSet<_>>()
    } else {
        catalog
            .official_names
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>()
    };
    let mut seen = HashSet::new();
    let mut aur_raw = 0usize;
    let mut aur_rendered = 0usize;
    for name in catalog.aur_names.lines() {
        aur_raw += name.len();
        if !crate::sources::valid_package_name(name)
            || official.contains(name)
            || !seen.insert(name)
        {
            continue;
        }
        if rows_started {
            output.write_all(b"\n")?;
        }
        let padding = 35usize.saturating_sub(UnicodeWidthStr::width(name));
        let installed = if catalog.installed.contains(name) {
            format!(
                " \x1b[32m{}\x1b[0m",
                crate::locale::text(crate::locale::current(), "package.installed", &[])
            )
        } else {
            String::new()
        };
        write!(
            output,
            "PKG:aur\t\x1b[35m{:<16}\x1b[0m\t\x1b[1m{name}\x1b[0m",
            "aur"
        )?;
        output.write_all(&SPACES[..padding.min(SPACES.len())])?;
        output.write_all(b"\t-                   ")?;
        output.write_all(installed.as_bytes())?;
        aur_rendered += name.len() + padding + installed.len() + 24;
        rows_started = true;
    }
    if std::env::var_os("SYSTEM_TOOLS_DEBUG_TIMINGS").is_some() {
        eprintln!(
            "render_timing source=aur_install_rows raw_bytes={} filtered_names={} rendered_bytes={}",
            aur_raw,
            seen.len(),
            aur_rendered
        );
    }
    Ok(())
}

pub(crate) fn write_package_row<W: Write + ?Sized>(
    output: &mut W,
    record: &PackageRecord,
    mode: PackageListMode,
) -> io::Result<()> {
    let color = super::source_color(record.source);
    let installed = if mode == PackageListMode::Install && record.installed {
        format!(
            " \x1b[32m{}\x1b[0m",
            crate::locale::text(crate::locale::current(), "package.installed", &[])
        )
    } else {
        String::new()
    };
    let localized_source;
    let source = if let Some(repository) = record.repository.as_deref() {
        if repository == "flatpak@system" {
            "flatpak@system"
        } else {
            repository
        }
    } else {
        localized_source = crate::locale::source_label(crate::locale::current(), record.source);
        localized_source.as_str()
    };
    let internal = match record.source {
        PackageSource::Flatpak if record.repository.as_deref() == Some("flatpak@system") => {
            "FLTK:flatpak@system".to_owned()
        }
        PackageSource::Flatpak => match &record.listing {
            PackageListing::Flatpak { origin, .. } if !origin.is_empty() => {
                format!("FLTK:{origin}")
            }
            _ => "FLTK:flatpak".to_owned(),
        },
        PackageSource::Aur => "PKG:aur".to_owned(),
        PackageSource::Pacman => {
            format!("PKG:{}", record.repository.as_deref().unwrap_or("pacman"))
        }
    };
    let source_padding = 16usize.saturating_sub(UnicodeWidthStr::width(source));
    let name_padding = 35usize.saturating_sub(UnicodeWidthStr::width(record.name.as_str()));
    if mode == PackageListMode::Install {
        write!(output, "{internal}\t\x1b[{color}m{source}")?;
        output.write_all(&SPACES[..source_padding.min(SPACES.len())])?;
        output.write_all(b"\x1b[0m")?;
        write!(output, "\t\x1b[1m{}\x1b[0m", record.name)?;
    } else {
        write!(output, "{internal}\t\x1b[{color}m{source}")?;
        output.write_all(&SPACES[..source_padding.min(SPACES.len())])?;
        output.write_all(b"\x1b[0m")?;
        write!(output, "\t{}", record.name)?;
    }
    output.write_all(&SPACES[..name_padding.min(SPACES.len())])?;
    output.write_all(b"\t")?;
    match &record.listing {
        PackageListing::Version(version) => {
            if mode == PackageListMode::Install {
                output.write_all(b"\x1b[2m")?;
            }
            output.write_all(version.as_bytes())?;
            if mode == PackageListMode::Install {
                output.write_all(b"\x1b[0m")?;
            }
            let padding = 20usize.saturating_sub(UnicodeWidthStr::width(version.as_str()));
            output.write_all(&SPACES[..padding.min(SPACES.len())])?;
        }
        PackageListing::Flatpak { app_name, origin } => {
            write!(output, "{app_name} ({origin})")?;
        }
    }
    output.write_all(installed.as_bytes())
}

pub(crate) fn parse_package_row(row: &str) -> Option<PackageRow> {
    let stripped = strip_ansi(row);
    let clean = strip_outer_quotes(&stripped);
    let (source_label, name, hidden) = if let Some((source, rest)) = clean.split_once('\t') {
        let hidden = source.trim().starts_with("PKG:") || source.trim().starts_with("FLTK:");
        if hidden {
            let mut fields = rest.split('\t');
            let visible = fields.next()?.trim();
            let name = fields
                .next()
                .and_then(|value| value.split_whitespace().next())
                .unwrap_or_else(|| visible.split_whitespace().next().unwrap_or_default());
            (source.trim(), name, true)
        } else {
            (source.trim(), rest.split_whitespace().next()?, false)
        }
    } else {
        let mut fields = clean.split_whitespace();
        (fields.next()?, fields.next()?, false)
    };
    if name.is_empty() {
        return None;
    }
    let (source_label, scope) = if hidden {
        match source_label.strip_prefix("FLTK:") {
            Some(origin) => (
                "flatpak",
                if origin == "flatpak@system" {
                    "system"
                } else {
                    "user"
                },
            ),
            None => (
                source_label.strip_prefix("PKG:").unwrap_or(source_label),
                "user",
            ),
        }
    } else {
        source_label
            .split_once('@')
            .unwrap_or((source_label, "user"))
    };
    if hidden && source_label == "aur" {
        return Some(PackageRow {
            source: PackageSource::Aur,
            repository: Some("aur".to_owned()),
            name: name.to_owned(),
        });
    }
    if hidden && source_label != "flatpak" {
        return Some(PackageRow {
            source: PackageSource::Pacman,
            repository: Some(source_label.to_owned()),
            name: name.to_owned(),
        });
    }
    let (source, repository) = match PackageSource::parse(source_label).or_else(|| {
        [
            PackageSource::Pacman,
            PackageSource::Aur,
            PackageSource::Flatpak,
        ]
        .into_iter()
        .find(|source| {
            crate::locale::source_label(crate::locale::Lang::En, *source) == source_label
                || crate::locale::source_label(crate::locale::Lang::Zh, *source) == source_label
        })
    }) {
        Some(source) => (
            source,
            (source == PackageSource::Flatpak && scope == "system")
                .then(|| "flatpak@system".to_owned()),
        ),
        None if source_label == "flatpak" => (
            PackageSource::Flatpak,
            (scope == "system").then(|| "flatpak@system".to_owned()),
        ),
        None if crate::sources::valid_package_name(source_label) => {
            (PackageSource::Pacman, Some(source_label.to_owned()))
        }
        None => return None,
    };
    Some(PackageRow {
        source,
        repository,
        name: name.to_owned(),
    })
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

#[cfg(test)]
mod tests {
    use super::{PackageRow, parse_package_row};
    use crate::model::{PackageListing, PackageRecord};
    use std::collections::HashSet;
    use system_tools_core::PackageSource;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn parses_ansi_padded_tab_row() {
        let row = "\x1b[34mcore            \x1b[0m\tbash                               \t5.2-1";
        assert_eq!(
            parse_package_row(row),
            Some(PackageRow {
                source: PackageSource::Pacman,
                repository: Some("core".to_owned()),
                name: "bash".to_owned(),
            })
        );
    }

    #[test]
    fn parses_space_aligned_downgrade_row() {
        assert_eq!(
            parse_package_row("extra            bash                           5.2-1"),
            Some(PackageRow {
                source: PackageSource::Pacman,
                repository: Some("extra".to_owned()),
                name: "bash".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_invalid_source_labels() {
        assert_eq!(parse_package_row("../escape bash 5.2-1"), None);
    }

    #[test]
    fn parses_fzf_shell_quoted_row() {
        let row = "'core            \tbash                               \t5.3-1                [已安装]'";
        assert_eq!(
            parse_package_row(row),
            Some(PackageRow {
                source: PackageSource::Pacman,
                repository: Some("core".to_owned()),
                name: "bash".to_owned(),
            })
        );
    }

    #[test]
    fn hidden_source_fields_round_trip_without_becoming_visible_identity() {
        assert_eq!(
            parse_package_row("PKG:core\tcore\t\x1b[1mbash\x1b[0m\t5.3-1"),
            Some(PackageRow {
                source: PackageSource::Pacman,
                repository: Some("core".to_owned()),
                name: "bash".to_owned(),
            })
        );
        assert_eq!(
            parse_package_row("PKG:aur\taur\ttool\t-"),
            Some(PackageRow {
                source: PackageSource::Aur,
                repository: Some("aur".to_owned()),
                name: "tool".to_owned(),
            })
        );
        assert_eq!(
            parse_package_row("FLTK:flathub\tFlatpak\torg.example.App\tDemo"),
            Some(PackageRow {
                source: PackageSource::Flatpak,
                repository: None,
                name: "org.example.App".to_owned(),
            })
        );
    }

    #[test]
    fn pads_cjk_names_by_terminal_columns() {
        let record = PackageRecord {
            source: PackageSource::Flatpak,
            repository: None,
            name: "示例应用".to_owned(),
            listing: PackageListing::Version("1.0".to_owned()),
            installed: true,
        };
        let row = super::render_package_rows(&[record], super::PackageListMode::Remove);
        let clean = super::super::strip_ansi(&row);
        let name_column = clean.split('\t').nth(2).expect("name column");
        assert_eq!(UnicodeWidthStr::width(name_column), 35);
    }

    #[test]
    fn install_emphasis_does_not_leak_into_remove_rows() {
        let record = PackageRecord {
            source: PackageSource::Pacman,
            repository: Some("core".to_owned()),
            name: "bash".to_owned(),
            listing: PackageListing::Version("5.3-1".to_owned()),
            installed: true,
        };
        let install = super::render_package_rows(&[record], super::PackageListMode::Install);
        let remove = super::render_package_rows(
            &[PackageRecord {
                source: PackageSource::Pacman,
                repository: Some("core".to_owned()),
                name: "bash".to_owned(),
                listing: PackageListing::Version("5.3-1".to_owned()),
                installed: true,
            }],
            super::PackageListMode::Remove,
        );

        assert!(install.contains("\t\x1b[1mbash\x1b[0m"));
        assert!(install.contains("\x1b[2m5.3-1\x1b[0m"));
        assert!(!remove.contains("\x1b[1mbash\x1b[0m"));
        assert!(!remove.contains("\x1b[2m5.3-1\x1b[0m"));
        assert!(remove.contains("\tbash"));
        assert!(!remove.contains("[已安装]") && !remove.contains("[Installed]"));
    }

    #[test]
    fn preserves_long_package_values_for_fzf_accept() {
        let record = PackageRecord {
            source: PackageSource::Pacman,
            repository: Some("community-long-name".to_owned()),
            name: "package-name-that-is-longer-than-the-display-column".to_owned(),
            listing: PackageListing::Version("version-with-a-long-suffix-2026.09.27-1".to_owned()),
            installed: false,
        };
        let row = super::render_package_rows(&[record], super::PackageListMode::Install);
        let plain = super::super::strip_ansi(&row);
        assert!(plain.contains("package-name-that-is-longer-than-the-display-column"));
        assert!(plain.contains("version-with-a-long-suffix-2026.09.27-1"));
    }

    #[test]
    fn install_catalog_skips_official_and_duplicate_aur_names() {
        let catalog = crate::sources::InstallCatalog {
            official: vec![PackageRecord {
                source: PackageSource::Pacman,
                repository: Some("core".to_owned()),
                name: "bash".to_owned(),
                listing: PackageListing::Version("5.3".to_owned()),
                installed: false,
            }],
            official_names: HashSet::new(),
            aur_names: "bash\ntool\ntool\n../invalid\n".to_owned(),
            installed: HashSet::new(),
        };
        let mut bytes = Vec::new();
        super::write_install_catalog(&catalog, &mut bytes).expect("render catalog");
        let rows = String::from_utf8(bytes).expect("UTF-8 catalog");
        let plain = super::super::strip_ansi(&rows);
        assert_eq!(plain.matches("\ttool").count(), 1);
        assert_eq!(plain.matches("\tbash").count(), 1);
        assert!(plain.contains("PKG:aur\t"));
        assert!(!rows.contains("invalid"));
    }
}
