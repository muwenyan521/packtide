use system_tools_core::{
    BackendClass, BackendId, BuiltinBackend, CapabilitySet, CatalogStrategy, PackageBackend,
    PackageKind, PackageScope,
};

fn read_capabilities() -> CapabilitySet {
    CapabilitySet::CATALOG
        .union(CapabilitySet::SEARCH)
        .union(CapabilitySet::INSTALLED)
        .union(CapabilitySet::DETAILS)
        .union(CapabilitySet::UPDATES)
}

fn write_capabilities() -> CapabilitySet {
    read_capabilities()
        .union(CapabilitySet::INSTALL)
        .union(CapabilitySet::REMOVE)
        .union(CapabilitySet::UPGRADE)
}

#[test]
fn backend_capability_matrix() {
    let read = read_capabilities();
    let write = write_capabilities();
    let install_remove_system_upgrade = read
        .union(CapabilitySet::INSTALL)
        .union(CapabilitySet::REMOVE)
        .union(CapabilitySet::SYSTEM_UPGRADE);
    let refresh = read.union(CapabilitySet::REFRESH_CATALOG);
    let full_write = write
        .union(CapabilitySet::DOWNGRADE)
        .union(CapabilitySet::SYSTEM_UPGRADE);
    let matrix = [
        (
            BackendId::Pacman,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            full_write,
        ),
        (
            BackendId::Apt,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            write
                .union(CapabilitySet::SYSTEM_UPGRADE)
                .union(CapabilitySet::REFRESH_CATALOG),
        ),
        (
            BackendId::Dnf5,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            write.union(CapabilitySet::SYSTEM_UPGRADE),
        ),
        (
            BackendId::Dnf4,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            write.union(CapabilitySet::SYSTEM_UPGRADE),
        ),
        (
            BackendId::Zypper,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            refresh
                .union(CapabilitySet::INSTALL)
                .union(CapabilitySet::REMOVE)
                .union(CapabilitySet::UPGRADE)
                .union(CapabilitySet::SYSTEM_UPGRADE),
        ),
        (
            BackendId::Apk,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            install_remove_system_upgrade.union(CapabilitySet::REFRESH_CATALOG),
        ),
        (
            BackendId::Xbps,
            BackendClass::Native,
            PackageKind::System,
            PackageScope::System,
            CatalogStrategy::Enumerated,
            install_remove_system_upgrade.union(CapabilitySet::REFRESH_CATALOG),
        ),
        (
            BackendId::Paru,
            BackendClass::Optional,
            PackageKind::Aur,
            PackageScope::User,
            CatalogStrategy::Enumerated,
            full_write,
        ),
        (
            BackendId::Yay,
            BackendClass::Optional,
            PackageKind::Aur,
            PackageScope::User,
            CatalogStrategy::Enumerated,
            full_write,
        ),
        (
            BackendId::Flatpak,
            BackendClass::Optional,
            PackageKind::Flatpak,
            PackageScope::User,
            CatalogStrategy::Enumerated,
            write.union(CapabilitySet::REFRESH_CATALOG),
        ),
        (
            BackendId::Snap,
            BackendClass::Optional,
            PackageKind::Snap,
            PackageScope::System,
            CatalogStrategy::DirectQuery,
            write.union(CapabilitySet::SYSTEM_UPGRADE),
        ),
        (
            BackendId::Brew,
            BackendClass::Optional,
            PackageKind::BrewFormula,
            PackageScope::Profile,
            CatalogStrategy::Enumerated,
            write,
        ),
        (
            BackendId::Nix,
            BackendClass::Optional,
            PackageKind::Nix,
            PackageScope::Profile,
            CatalogStrategy::DirectQuery,
            write,
        ),
    ];

    assert_eq!(
        BackendId::ALL,
        matrix.map(|(backend, ..)| backend),
        "matrix must cover every backend exactly once in declaration order"
    );
    assert_eq!(
        PackageKind::ALL,
        [
            PackageKind::System,
            PackageKind::Aur,
            PackageKind::Flatpak,
            PackageKind::Snap,
            PackageKind::BrewFormula,
            PackageKind::BrewCask,
            PackageKind::Nix,
        ]
    );
    assert_eq!(
        PackageScope::ALL,
        [
            PackageScope::System,
            PackageScope::User,
            PackageScope::Profile
        ]
    );

    for (backend, class, kind, scope, strategy, capabilities) in matrix {
        let provider = BuiltinBackend::new(backend);
        assert_eq!(provider.id(), backend, "{backend:?} id");
        assert_eq!(provider.class(), class, "{backend:?} class");
        assert_eq!(provider.kind(), kind, "{backend:?} kind");
        assert_eq!(provider.scope(), scope, "{backend:?} scope");
        assert_eq!(
            provider.catalog_strategy(),
            strategy,
            "{backend:?} catalog strategy"
        );
        assert_eq!(
            provider.capabilities().bits(),
            capabilities.bits(),
            "{backend:?} facade read/write capability bits"
        );
    }
}
