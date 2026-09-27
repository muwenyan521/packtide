use super::cli::{Cli, CommandLine};
use super::mirror_update::fallback_region;
use super::model::{PackageListing, PackageRecord};
use super::model::{UpdateSource, parse_cached_updates, parse_flatpak, parse_updates};
use super::sources::{parse_install_rows, parse_remove_rows, valid_package_name};
use super::ui::{PackageListMode, render_package_rows, strip_ansi};
use clap::Parser;
use system_tools_core::PackageSource;

#[test]
fn parses_repository_update_lines_without_losing_raw_text() {
    let updates = parse_updates(
        PackageSource::Pacman,
        "linux 6.12.1 -> 6.12.2\nabseil-cpp 20260817.0-1 -> 20260817.0-2\n",
    );
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].source, UpdateSource::Pacman);
    assert_eq!(updates[0].name, "linux");
    assert_eq!(updates[1].name, "abseil-cpp");
}

#[test]
fn parses_flatpak_application_and_version() {
    let updates = parse_flatpak("org.example.App 1.2.3\n");
    assert_eq!(updates[0].source, UpdateSource::Flatpak);
    assert_eq!(updates[0].name, "org.example.App");
    assert_eq!(updates[0].version.as_deref(), Some("1.2.3"));
}

#[test]
fn cache_round_trip_preserves_display_text() {
    let updates = parse_cached_updates("pacman\tlinux 6.12 -> 6.13\naur\ttool-git 1 -> 2");
    assert_eq!(updates[0].display, "linux 6.12 -> 6.13");
    assert_eq!(updates[1].source, UpdateSource::Aur);
    assert!(parse_cached_updates("core\tbash 5.2 -> 5.3").is_empty());
}

#[test]
fn selects_regional_mirror_fallbacks() {
    assert_eq!(
        fallback_region("China").as_deref(),
        Some("China,Hong Kong,Taiwan,Japan,Singapore")
    );
    assert_eq!(fallback_region("Unknown"), None);
}

#[test]
fn keeps_aur_and_flatpak_removal_rows_distinct() {
    let records = parse_remove_rows(
        "bash 5.2\ncustom-tool 1.0\n",
        "core bash 5.2\n",
        "org.example.App\tflathub\tExample App\n",
    );
    assert_eq!(
        records,
        vec![
            PackageRecord {
                source: PackageSource::Pacman,
                repository: None,
                name: "bash".to_owned(),
                listing: PackageListing::Version("5.2".to_owned()),
                installed: true,
            },
            PackageRecord {
                source: PackageSource::Aur,
                repository: None,
                name: "custom-tool".to_owned(),
                listing: PackageListing::Version("1.0".to_owned()),
                installed: true,
            },
            PackageRecord {
                source: PackageSource::Flatpak,
                repository: None,
                name: "org.example.App".to_owned(),
                listing: PackageListing::Flatpak {
                    app_name: "Example App".to_owned(),
                    origin: "flathub".to_owned(),
                },
                installed: true,
            },
        ]
    );
    let rows = render_package_rows(&records, PackageListMode::Remove);
    assert!(rows.contains("\x1b[36mflatpak         \x1b[0m\torg.example.App"));
    assert!(rows.contains("Example App (flathub)"));
    assert!(!rows.contains("[已安装]"));
    let flatpak_row = rows
        .lines()
        .find(|row| row.contains("org.example.App"))
        .expect("rendered Flatpak row");
    let selected = super::ui::parse_package_row(flatpak_row).expect("parsed Flatpak row");
    assert_eq!(selected.source, PackageSource::Flatpak);
    assert_eq!(selected.name, "org.example.App");
}

#[test]
fn parses_install_rows_with_padding_and_installed_state() {
    let installed = ["bash".to_owned(), "abseil-cpp".to_owned()]
        .into_iter()
        .collect();
    let records = parse_install_rows("core bash 5.3-1\nextra abseil-cpp 1.0-1\n", &installed);
    assert!(records.iter().all(|record| record.installed));
    assert_eq!(records[0].source, PackageSource::Pacman);
    assert_eq!(records[0].repository.as_deref(), Some("core"));
    assert_eq!(records[0].name, "bash");
    assert_eq!(
        records[0].listing,
        PackageListing::Version("5.3-1".to_owned())
    );
    assert_eq!(records[1].name, "abseil-cpp");
    assert_eq!(
        records[1].listing,
        PackageListing::Version("1.0-1".to_owned())
    );
}

#[test]
fn renders_package_records_with_ui_owned_ansi_and_columns() {
    let records = vec![PackageRecord {
        source: PackageSource::Pacman,
        repository: Some("core".to_owned()),
        name: "bash".to_owned(),
        listing: PackageListing::Version("5.3-1".to_owned()),
        installed: true,
    }];

    let rows = render_package_rows(&records, PackageListMode::Install);

    assert_eq!(
        rows,
        format!(
            "\x1b[34mcore            \x1b[0m\t\x1b[1mbash\x1b[0m                               \t\x1b[2m5.3-1\x1b[0m                \x1b[32m{}\x1b[0m",
            crate::locale::text(crate::locale::current(), "package.installed", &[])
        )
    );
    assert_eq!(strip_ansi(&rows).split('\t').count(), 3);
}

#[test]
fn validates_package_names_at_the_boundary() {
    assert!(valid_package_name("python-pyproject-hooks"));
    assert!(valid_package_name("foo+bar"));
    assert!(!valid_package_name(""));
    assert!(!valid_package_name("../escape"));
    assert!(!valid_package_name("name with spaces"));
}

#[test]
fn parses_explicit_cli_commands_and_compatibility_flags() {
    let cli = Cli::try_parse_from(["packtide", "install", "--refresh", "query"])
        .expect("parse install command");
    assert!(
        matches!(cli.command, Some(CommandLine::Install { refresh: true, query, .. }) if query == vec!["query"])
    );
    let cli = Cli::try_parse_from(["packtide", "remove", "query"]).expect("parse remove command");
    assert!(matches!(cli.command, Some(CommandLine::Remove { query }) if query == vec!["query"]));
}
