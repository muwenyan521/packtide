use super::strip_ansi;
use crate::model::{PackageListing, PackageRecord};
use crate::source_metadata;
use std::collections::HashSet;
use std::io::{self, Write};
use system_tools_core::{
    BackendId, NativePackageKey, PackageIdentity, PackageKind, PackageScope, PackageSource,
};
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
        let record = PackageRecord::legacy(
            PackageSource::Aur,
            Some("aur".to_owned()),
            name.to_owned(),
            PackageListing::Version("-".to_owned()),
            catalog.installed.contains(name),
        );
        let mut rendered = Vec::new();
        write_package_row(&mut rendered, &record, PackageListMode::Install)?;
        output.write_all(&rendered)?;
        aur_rendered += rendered.len();
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
            "\x1b[32m{}\x1b[0m",
            crate::locale::text(crate::locale::current(), "package.installed", &[])
        )
    } else {
        String::new()
    };
    let internal = hidden_source_token(record);
    let source = source_metadata::record_label(record);
    let source_padding = 16usize.saturating_sub(UnicodeWidthStr::width(source.as_str()));
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
    output.write_all(b"\t")?;
    output.write_all(installed.as_bytes())
}

fn hidden_source_token(record: &PackageRecord) -> String {
    let base = source_metadata::hidden_token(record);
    let identity = record.identity();
    let simple_identity = identity.backend == record.source.backend_for_source()
        && identity.kind == record.source.default_kind()
        && identity.scope
            == source_metadata::legacy_scope(record.source, record.repository.as_deref())
        && identity.native_key.as_str() == record.name
        && identity.origin.is_none()
        && identity.display_name.is_none();
    if simple_identity {
        return base;
    }
    format!(
        "{base}|b={}|s={}|k={}|n={}|r={}|o={}|d={}",
        identity.backend.as_str(),
        identity.scope.as_str(),
        identity.kind.as_str(),
        encode_text(identity.native_key.as_str()),
        encode_optional(record.repository.as_deref()),
        encode_optional(identity.origin.as_deref()),
        encode_optional(identity.display_name.as_deref()),
    )
}

fn encode_optional(value: Option<&str>) -> String {
    value.map_or_else(String::new, encode_text)
}

fn encode_text(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn decode_text(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

pub(crate) fn parse_package_row(row: &str) -> Option<PackageRow> {
    let stripped = strip_ansi(row);
    let clean = strip_outer_quotes(&stripped);
    let (source_label, name, hidden) = if let Some((source, rest)) = clean.split_once('\t') {
        let hidden = source
            .trim()
            .split_once(':')
            .is_some_and(|(prefix, _)| prefix.chars().all(|c| c.is_ascii_uppercase()));
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
    let token_source_label = source_label
        .split_once('|')
        .map_or(source_label, |(head, _)| head);
    if hidden {
        let (source, detail) = PackageSource::parse_hidden_token(token_source_label)?;
        return Some(PackageRow {
            source,
            repository: source_metadata::hidden_repository(source, detail),
            name: name.to_owned(),
        });
    }
    let (source_label, scope) = source_label
        .split_once('@')
        .unwrap_or((source_label, "user"));
    let (source, repository) = match source_metadata::parse_label(source_label) {
        Some(source) => (source, source_metadata::legacy_repository(source, scope)),
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

pub(crate) fn parse_package_identity(row: &str) -> Option<PackageIdentity> {
    let parsed = parse_package_row(row)?;
    let stripped = strip_ansi(row);
    let clean = strip_outer_quotes(&stripped);
    let token = clean.split('\t').next()?.trim();
    if !token.contains(':') {
        let backend = parsed.source.backend_for_source();
        let scope = source_metadata::legacy_scope(parsed.source, parsed.repository.as_deref());
        let key = source_metadata::legacy_native_key(
            parsed.source,
            parsed.repository.as_deref(),
            &parsed.name,
        );
        return Some(PackageIdentity::new(
            backend,
            parsed.source.default_kind(),
            scope,
            NativePackageKey::new(key).ok()?,
        ));
    }
    let token = token.split_once('|').map_or(token, |(head, _)| head);
    let (source, detail) = PackageSource::parse_hidden_token(token)?;
    let mut backend = source.backend_for_source();
    let mut scope = source_metadata::legacy_scope(source, parsed.repository.as_deref());
    let mut kind = source.default_kind();
    let mut native_key = NativePackageKey::new(parsed.name.clone()).ok()?;
    let mut origin = source_metadata::hidden_origin(source, detail);
    let mut display_name = None;
    let raw_token = clean.split('\t').next()?.trim();
    let attributes = raw_token
        .split_once('|')
        .map_or("", |(_, attributes)| attributes);
    for attribute in attributes
        .split('|')
        .filter(|attribute| !attribute.is_empty())
    {
        let (key, value) = attribute.split_once('=')?;
        match key {
            "b" => backend = BackendId::parse(value)?,
            "s" => scope = PackageScope::parse(value)?,
            "k" => kind = PackageKind::parse(value)?,
            "n" => native_key = NativePackageKey::new(decode_text(value)?).ok()?,
            "o" => origin = (!value.is_empty()).then(|| decode_text(value)).flatten(),
            "d" => display_name = (!value.is_empty()).then(|| decode_text(value)).flatten(),
            "r" => {}
            _ => return None,
        }
    }
    if backend.package_source() != parsed.source || !backend.supports_kind(kind) {
        return None;
    }
    let mut identity = PackageIdentity::new(backend, kind, scope, native_key);
    if let Some(origin) = origin {
        identity = identity.with_origin(origin);
    }
    if let Some(display_name) = display_name {
        identity = identity.with_display_name(display_name);
    }
    Some(identity)
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
    use super::{PackageRow, parse_package_identity, parse_package_row};
    use crate::model::{PackageListing, PackageRecord};
    use std::collections::HashSet;
    use system_tools_core::{
        BackendId, NativePackageKey, PackageIdentity, PackageKind, PackageScope, PackageSource,
    };
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
        assert_eq!(
            parse_package_row("APT:main\tAPT\tbash\t5.2"),
            Some(PackageRow {
                source: PackageSource::Apt,
                repository: Some("main".to_owned()),
                name: "bash".to_owned(),
            })
        );
    }

    #[test]
    fn legacy_flatpak_tokens_keep_scope_and_remote_when_restoring_identity() {
        let cases = [
            ("FLTK:flatpak", PackageScope::User, None),
            ("FLTK:flathub", PackageScope::User, Some("flathub")),
            ("FLTK:flatpak@system", PackageScope::System, None),
        ];

        let identities = cases.map(|(token, _, _)| {
            parse_package_identity(&format!("{token}\tFlatpak\torg.example.App\tDemo"))
        });

        for (identity, (_, scope, origin)) in identities.into_iter().zip(cases) {
            assert_eq!(
                identity,
                Some(PackageIdentity {
                    backend: BackendId::Flatpak,
                    kind: PackageKind::Flatpak,
                    scope,
                    native_key: NativePackageKey::new("org.example.App").unwrap(),
                    origin: origin.map(str::to_owned),
                    display_name: None,
                })
            );
        }
    }

    #[test]
    fn pads_cjk_names_by_terminal_columns() {
        let record = PackageRecord::legacy(
            PackageSource::Flatpak,
            None,
            "示例应用".to_owned(),
            PackageListing::Version("1.0".to_owned()),
            true,
        );
        let row = super::render_package_rows(&[record], super::PackageListMode::Remove);
        let clean = super::super::strip_ansi(&row);
        let name_column = clean.split('\t').nth(2).expect("name column");
        assert_eq!(UnicodeWidthStr::width(name_column), 35);
    }

    #[test]
    fn install_emphasis_does_not_leak_into_remove_rows() {
        let record = PackageRecord::legacy(
            PackageSource::Pacman,
            Some("core".to_owned()),
            "bash".to_owned(),
            PackageListing::Version("5.3-1".to_owned()),
            true,
        );
        let install = super::render_package_rows(&[record], super::PackageListMode::Install);
        let remove = super::render_package_rows(
            &[PackageRecord::legacy(
                PackageSource::Pacman,
                Some("core".to_owned()),
                "bash".to_owned(),
                PackageListing::Version("5.3-1".to_owned()),
                true,
            )],
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
    fn rendered_rows_keep_status_in_a_fixed_fifth_column() {
        for mode in [
            super::PackageListMode::Install,
            super::PackageListMode::Remove,
        ] {
            for installed in [false, true] {
                let record = PackageRecord::legacy(
                    PackageSource::Pacman,
                    Some("core".to_owned()),
                    "bash".to_owned(),
                    PackageListing::Version("5.3-1".to_owned()),
                    installed,
                );

                let row = super::render_package_rows(&[record], mode);

                let clean = super::super::strip_ansi(&row);
                let fields = clean.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 5, "mode={mode:?} installed={installed}");
                assert_eq!(fields[3].trim(), "5.3-1");
                let expected = if mode == super::PackageListMode::Install && installed {
                    crate::locale::text(crate::locale::current(), "package.installed", &[])
                } else {
                    String::new()
                };
                assert_eq!(fields[4].trim(), expected);
                assert_eq!(parse_package_row(&row).unwrap().name, "bash");
            }
        }
    }

    #[test]
    fn identity_parser_rejects_kinds_from_another_backend() {
        for (backend, kind) in [
            (BackendId::Brew, PackageKind::System),
            (BackendId::Apt, PackageKind::BrewCask),
        ] {
            let record = PackageRecord::from_identity(
                PackageIdentity::new(
                    backend,
                    kind,
                    backend.default_scope(),
                    NativePackageKey::new("native-key").unwrap(),
                ),
                None,
                "display-name".to_owned(),
                PackageListing::Version("1.0".to_owned()),
                false,
            );

            let row = super::render_package_rows(&[record], super::PackageListMode::Install);

            assert_eq!(parse_package_identity(&row), None);
        }
    }

    #[test]
    fn preserves_long_package_values_for_fzf_accept() {
        let record = PackageRecord::legacy(
            PackageSource::Pacman,
            Some("community-long-name".to_owned()),
            "package-name-that-is-longer-than-the-display-column".to_owned(),
            PackageListing::Version("version-with-a-long-suffix-2026.09.27-1".to_owned()),
            false,
        );
        let row = super::render_package_rows(&[record], super::PackageListMode::Install);
        let plain = super::super::strip_ansi(&row);
        assert!(plain.contains("package-name-that-is-longer-than-the-display-column"));
        assert!(plain.contains("version-with-a-long-suffix-2026.09.27-1"));
    }

    #[test]
    fn install_catalog_skips_official_and_duplicate_aur_names() {
        let catalog = crate::sources::InstallCatalog {
            official: vec![PackageRecord::legacy(
                PackageSource::Pacman,
                Some("core".to_owned()),
                "bash".to_owned(),
                PackageListing::Version("5.3".to_owned()),
                false,
            )],
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

    #[test]
    fn rendered_rows_round_trip_backend_scope_kind_key_and_origin() {
        let cases = [
            (
                BackendId::Pacman,
                PackageKind::System,
                PackageScope::System,
                None,
                None,
                Some("core"),
            ),
            (
                BackendId::Apt,
                PackageKind::System,
                PackageScope::System,
                None,
                None,
                Some("main"),
            ),
            (
                BackendId::Zypper,
                PackageKind::System,
                PackageScope::System,
                None,
                None,
                Some("repo-oss"),
            ),
            (
                BackendId::Apk,
                PackageKind::System,
                PackageScope::System,
                None,
                None,
                Some("main"),
            ),
            (
                BackendId::Xbps,
                PackageKind::System,
                PackageScope::System,
                None,
                None,
                Some("current"),
            ),
            (
                BackendId::Dnf4,
                PackageKind::System,
                PackageScope::System,
                None,
                None,
                Some("fedora"),
            ),
            (
                BackendId::Yay,
                PackageKind::Aur,
                PackageScope::User,
                None,
                None,
                Some("aur"),
            ),
            (
                BackendId::Flatpak,
                PackageKind::Flatpak,
                PackageScope::User,
                Some("flathub"),
                Some("Demo App"),
                None,
            ),
            (
                BackendId::Snap,
                PackageKind::Snap,
                PackageScope::System,
                None,
                None,
                Some("stable"),
            ),
            (
                BackendId::Brew,
                PackageKind::BrewFormula,
                PackageScope::Profile,
                None,
                None,
                Some("formula"),
            ),
            (
                BackendId::Brew,
                PackageKind::BrewCask,
                PackageScope::Profile,
                None,
                None,
                Some("cask"),
            ),
            (
                BackendId::Nix,
                PackageKind::Nix,
                PackageScope::Profile,
                None,
                None,
                Some("profile"),
            ),
        ];
        for (backend, kind, scope, origin, display_name, repository) in cases {
            let identity = PackageIdentity::new(
                backend,
                kind,
                scope,
                NativePackageKey::new(format!("{}/native-key", backend.as_str())).unwrap(),
            );
            let identity = match (origin, display_name) {
                (Some(origin), Some(display_name)) => {
                    identity.with_origin(origin).with_display_name(display_name)
                }
                (Some(origin), None) => identity.with_origin(origin),
                (None, Some(display_name)) => identity.with_display_name(display_name),
                (None, None) => identity,
            };
            let listing = if backend == BackendId::Flatpak {
                PackageListing::Flatpak {
                    app_name: "Demo App".to_owned(),
                    origin: "flathub".to_owned(),
                }
            } else {
                PackageListing::Version("1.0".to_owned())
            };
            let record = PackageRecord::from_identity(
                identity.clone(),
                repository.map(str::to_owned),
                "display-name".to_owned(),
                listing,
                false,
            );
            let row = super::render_package_rows(&[record], super::PackageListMode::Install);
            assert_eq!(
                parse_package_identity(&row),
                Some(identity),
                "identity lost for {backend:?}"
            );
        }
    }
}
