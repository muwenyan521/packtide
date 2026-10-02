use std::collections::HashSet;
use system_tools_core::PackageSource;

use crate::model::{PackageListing, PackageRecord};

use super::valid_package_name;

#[cfg(test)]
pub(crate) fn parse_install_rows(
    official: &str,
    installed: &HashSet<String>,
) -> Vec<PackageRecord> {
    official
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let repo = fields.next()?;
            let name = fields.next()?;
            let version = fields.next()?;
            if !valid_package_name(repo) || !valid_package_name(name) {
                return None;
            }
            Some(PackageRecord {
                source: PackageSource::Pacman,
                repository: Some(repo.to_owned()),
                name: name.to_owned(),
                listing: PackageListing::Version(version.to_owned()),
                installed: installed.contains(name),
            })
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn parse_install_line(line: &str, installed: &HashSet<String>) -> Option<PackageRecord> {
    let mut fields = line.split_whitespace();
    let repo = fields.next()?;
    let name = fields.next()?;
    let version = fields.next()?;
    if !valid_package_name(repo) || !valid_package_name(name) {
        return None;
    }
    Some(PackageRecord {
        source: PackageSource::Pacman,
        repository: Some(repo.to_owned()),
        name: name.to_owned(),
        listing: PackageListing::Version(version.to_owned()),
        installed: installed.contains(name),
    })
}

#[allow(dead_code)]
pub(crate) fn parse_remove_rows(installed: &str, sync: &str) -> Vec<PackageRecord> {
    let repositories = sync
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect::<HashSet<_>>();
    installed
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let version = fields.next().unwrap_or_default();
            let source = if repositories.contains(name) {
                PackageSource::Pacman
            } else {
                PackageSource::Aur
            };
            Some(PackageRecord {
                source,
                repository: None,
                name: name.to_owned(),
                listing: PackageListing::Version(version.to_owned()),
                installed: true,
            })
        })
        .collect()
}
