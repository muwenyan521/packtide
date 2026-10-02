pub(crate) use system_tools_core::PackageSource as UpdateSource;
use system_tools_core::PackageSource;

#[derive(Debug)]
pub(crate) struct PackageUpdate {
    pub(crate) source: UpdateSource,
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    pub(crate) display: String,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PackageRecord {
    pub(crate) source: PackageSource,
    pub(crate) repository: Option<String>,
    pub(crate) name: String,
    pub(crate) listing: PackageListing,
    pub(crate) installed: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PackageListing {
    Version(String),
    Flatpak { app_name: String, origin: String },
}

#[allow(dead_code)]
pub(crate) fn parse_updates(source: PackageSource, text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some(PackageUpdate {
                source,
                name: parts.next()?.to_owned(),
                version: parts.next_back().map(str::to_owned),
                display: line.to_owned(),
            })
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn parse_flatpak(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some(PackageUpdate {
                source: PackageSource::Flatpak,
                name: parts.next()?.to_owned(),
                version: parts.next().map(str::to_owned),
                display: line.to_owned(),
            })
        })
        .collect()
}

pub(crate) fn parse_cached_updates(contents: &str) -> Vec<PackageUpdate> {
    contents
        .lines()
        .filter_map(|line| {
            let (source, display) = line.split_once('\t')?;
            let mut parts = display.split_whitespace();
            Some(PackageUpdate {
                source: PackageSource::parse(source)?,
                name: parts.next()?.to_owned(),
                version: parts.next_back().map(str::to_owned),
                display: display.to_owned(),
            })
        })
        .collect()
}

pub(crate) fn render_update_rows(updates: &[PackageUpdate]) -> String {
    use std::fmt::Write as _;

    let capacity = updates
        .iter()
        .map(|item| item.name.len() + item.display.len() + 32)
        .sum();
    let mut rows = String::with_capacity(capacity);
    for (index, item) in updates.iter().enumerate() {
        if index > 0 {
            rows.push('\n');
        }
        let color = crate::ui::source_color(item.source);
        let source_label = crate::locale::source_label(crate::locale::current(), item.source);
        write!(
            rows,
            "\x1b[{color}m[{:<7}]\x1b[0m\t{}\t{}",
            source_label, item.name, item.display
        )
        .expect("writing update row to String cannot fail");
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{PackageUpdate, render_update_rows};
    use system_tools_core::PackageSource;

    #[test]
    fn update_rows_use_localized_source_labels() {
        let rows = render_update_rows(&[
            PackageUpdate {
                source: PackageSource::Pacman,
                name: "bash".to_owned(),
                version: Some("5.3".to_owned()),
                display: "bash 5.3".to_owned(),
            },
            PackageUpdate {
                source: PackageSource::Aur,
                name: "tool".to_owned(),
                version: Some("1.0".to_owned()),
                display: "tool 1.0".to_owned(),
            },
            PackageUpdate {
                source: PackageSource::Flatpak,
                name: "org.example.App".to_owned(),
                version: Some("2.0".to_owned()),
                display: "org.example.App 2.0".to_owned(),
            },
        ]);
        let plain = crate::ui::strip_ansi(&rows);
        let lang = crate::locale::current();
        for source in [
            PackageSource::Pacman,
            PackageSource::Aur,
            PackageSource::Flatpak,
        ] {
            let label = crate::locale::source_label(lang, source);
            assert!(plain.contains(&format!("[{label}")), "missing {label}");
        }
    }
}
