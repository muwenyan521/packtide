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
        write!(
            rows,
            "\x1b[{color}m[{:<7}]\x1b[0m\t{}\t{}",
            item.source.as_str(),
            item.name,
            item.display
        )
        .expect("writing update row to String cannot fail");
    }
    rows
}
