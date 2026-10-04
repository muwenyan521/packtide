use system_tools_core::{BackendId, PackageScope, PackageSource};

use crate::model::{PackageListing, PackageRecord};

const FLATPAK_SYSTEM: &str = "flatpak@system";

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum PackageRoute {
    ArchRepository,
    ArchForeign,
    Flatpak,
    Other,
}

pub(crate) fn package_route(backend: BackendId) -> PackageRoute {
    match backend.package_source() {
        PackageSource::Pacman => PackageRoute::ArchRepository,
        PackageSource::Aur => PackageRoute::ArchForeign,
        PackageSource::Flatpak => PackageRoute::Flatpak,
        PackageSource::Apt
        | PackageSource::Dnf
        | PackageSource::Zypper
        | PackageSource::Apk
        | PackageSource::Xbps
        | PackageSource::Snap
        | PackageSource::Brew
        | PackageSource::Nix => PackageRoute::Other,
    }
}

pub(crate) fn legacy_scope(source: PackageSource, repository: Option<&str>) -> PackageScope {
    if source == PackageSource::Flatpak && repository == Some(FLATPAK_SYSTEM) {
        PackageScope::System
    } else {
        source.default_scope()
    }
}

pub(crate) fn record_label(record: &PackageRecord) -> String {
    record.repository.as_deref().map_or_else(
        || crate::locale::source_label(crate::locale::current(), record.source),
        str::to_owned,
    )
}

pub(crate) fn hidden_token(record: &PackageRecord) -> String {
    let detail = match record.source {
        PackageSource::Aur => record.source.source_key(),
        PackageSource::Flatpak if record.repository.as_deref() == Some(FLATPAK_SYSTEM) => {
            FLATPAK_SYSTEM
        }
        PackageSource::Flatpak => match &record.listing {
            PackageListing::Flatpak { origin, .. } if !origin.is_empty() => origin,
            _ => record.source.source_key(),
        },
        source => record.repository.as_deref().unwrap_or(source.source_key()),
    };
    record.source.hidden_token(detail)
}

pub(crate) fn hidden_repository(source: PackageSource, detail: &str) -> Option<String> {
    if source == PackageSource::Flatpak && detail != FLATPAK_SYSTEM {
        None
    } else {
        Some(detail.to_owned())
    }
}

pub(crate) fn hidden_origin(source: PackageSource, detail: &str) -> Option<String> {
    (source == PackageSource::Flatpak && detail != source.source_key() && detail != FLATPAK_SYSTEM)
        .then(|| detail.to_owned())
}

pub(crate) fn parse_label(label: &str) -> Option<PackageSource> {
    PackageSource::parse(label).or_else(|| {
        PackageSource::ALL.into_iter().find(|source| {
            crate::locale::source_label(crate::locale::Lang::En, *source) == label
                || crate::locale::source_label(crate::locale::Lang::Zh, *source) == label
        })
    })
}

pub(crate) fn aur_backend(helper: &str) -> BackendId {
    BackendId::parse(helper)
        .filter(|backend| backend.package_source() == PackageSource::Aur)
        .unwrap_or_else(|| PackageSource::Aur.backend_for_source())
}
