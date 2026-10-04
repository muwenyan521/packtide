use crate::model::{PackageListing, PackageRecord};
use system_tools_core::PackageSource;

#[allow(dead_code)]
pub(crate) fn parse_remove_rows(flatpak: &str) -> Vec<PackageRecord> {
    flatpak
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next()?.trim();
            let origin = fields.next().unwrap_or_default().trim();
            let name = fields.next().unwrap_or_default().trim();
            let installation = fields.next().unwrap_or("user").trim();
            (!id.is_empty()).then(|| {
                PackageRecord::legacy(
                    PackageSource::Flatpak,
                    (installation == "system").then(|| "flatpak@system".to_owned()),
                    id.to_owned(),
                    PackageListing::Flatpak {
                        app_name: name.to_owned(),
                        origin: origin.to_owned(),
                    },
                    true,
                )
            })
        })
        .collect()
}
