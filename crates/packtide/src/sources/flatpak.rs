use crate::model::{PackageListing, PackageRecord};
use system_tools_core::PackageSource;

pub(crate) fn parse_remove_rows(flatpak: &str) -> Vec<PackageRecord> {
    flatpak
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next()?.trim();
            let origin = fields.next().unwrap_or_default().trim();
            let name = fields.next().unwrap_or_default().trim();
            (!id.is_empty()).then(|| PackageRecord {
                source: PackageSource::Flatpak,
                repository: None,
                name: id.to_owned(),
                listing: PackageListing::Flatpak {
                    app_name: name.to_owned(),
                    origin: origin.to_owned(),
                },
                installed: true,
            })
        })
        .collect()
}
