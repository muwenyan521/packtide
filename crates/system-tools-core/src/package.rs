use crate::backend::{BackendId, PackageKind, PackageScope};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PackageSource {
    Pacman,
    Aur,
    Flatpak,
    Apt,
    Dnf,
    Zypper,
    Apk,
    Xbps,
    Snap,
    Brew,
    Nix,
}

impl PackageSource {
    pub const ALL: [Self; 11] = [
        Self::Pacman,
        Self::Aur,
        Self::Flatpak,
        Self::Apt,
        Self::Dnf,
        Self::Zypper,
        Self::Apk,
        Self::Xbps,
        Self::Snap,
        Self::Brew,
        Self::Nix,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Aur => "aur",
            Self::Flatpak => "flatpak",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
            Self::Apk => "apk",
            Self::Xbps => "xbps",
            Self::Snap => "snap",
            Self::Brew => "brew",
            Self::Nix => "nix",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pacman" => Some(Self::Pacman),
            "aur" => Some(Self::Aur),
            "flatpak" => Some(Self::Flatpak),
            "apt" => Some(Self::Apt),
            "dnf" => Some(Self::Dnf),
            "zypper" => Some(Self::Zypper),
            "apk" => Some(Self::Apk),
            "xbps" => Some(Self::Xbps),
            "snap" => Some(Self::Snap),
            "brew" => Some(Self::Brew),
            "nix" => Some(Self::Nix),
            _ => None,
        }
    }

    pub const fn backend_for_source(self) -> BackendId {
        match self {
            Self::Pacman => BackendId::Pacman,
            Self::Aur => BackendId::Paru,
            Self::Flatpak => BackendId::Flatpak,
            Self::Apt => BackendId::Apt,
            Self::Dnf => BackendId::Dnf5,
            Self::Zypper => BackendId::Zypper,
            Self::Apk => BackendId::Apk,
            Self::Xbps => BackendId::Xbps,
            Self::Snap => BackendId::Snap,
            Self::Brew => BackendId::Brew,
            Self::Nix => BackendId::Nix,
        }
    }

    pub const fn default_scope(self) -> PackageScope {
        self.backend_for_source().default_scope()
    }

    pub const fn hidden_prefix(self) -> &'static str {
        match self {
            Self::Pacman | Self::Aur => "PKG",
            Self::Flatpak => "FLTK",
            Self::Apt => "APT",
            Self::Dnf => "DNF",
            Self::Zypper => "ZYPPER",
            Self::Apk => "APK",
            Self::Xbps => "XBPS",
            Self::Snap => "SNAP",
            Self::Brew => "BREW",
            Self::Nix => "NIX",
        }
    }

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Pacman => "source.pacman",
            Self::Aur => "source.aur",
            Self::Flatpak => "source.flatpak",
            Self::Apt => "source.apt",
            Self::Dnf => "source.dnf",
            Self::Zypper => "source.zypper",
            Self::Apk => "source.apk",
            Self::Xbps => "source.xbps",
            Self::Snap => "source.snap",
            Self::Brew => "source.brew",
            Self::Nix => "source.nix",
        }
    }

    pub const fn color_code(self) -> &'static str {
        match self {
            Self::Pacman => "34",
            Self::Aur => "35",
            Self::Flatpak => "36",
            Self::Apt => "32",
            Self::Dnf => "33",
            Self::Zypper => "31",
            Self::Apk => "92",
            Self::Xbps => "95",
            Self::Snap => "93",
            Self::Brew => "91",
            Self::Nix => "96",
        }
    }

    pub fn hidden_token(self, detail: &str) -> String {
        format!("{}:{}", self.hidden_prefix(), detail)
    }
}

impl BackendId {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pacman" => Some(Self::Pacman),
            "apt" => Some(Self::Apt),
            "dnf5" => Some(Self::Dnf5),
            "dnf4" => Some(Self::Dnf4),
            "zypper" => Some(Self::Zypper),
            "apk" => Some(Self::Apk),
            "xbps" => Some(Self::Xbps),
            "paru" => Some(Self::Paru),
            "yay" => Some(Self::Yay),
            "flatpak" => Some(Self::Flatpak),
            "snap" => Some(Self::Snap),
            "brew" => Some(Self::Brew),
            "nix" => Some(Self::Nix),
            _ => None,
        }
    }

    pub const fn package_source(self) -> PackageSource {
        match self {
            Self::Pacman => PackageSource::Pacman,
            Self::Apt => PackageSource::Apt,
            Self::Dnf5 | Self::Dnf4 => PackageSource::Dnf,
            Self::Zypper => PackageSource::Zypper,
            Self::Apk => PackageSource::Apk,
            Self::Xbps => PackageSource::Xbps,
            Self::Paru | Self::Yay => PackageSource::Aur,
            Self::Flatpak => PackageSource::Flatpak,
            Self::Snap => PackageSource::Snap,
            Self::Brew => PackageSource::Brew,
            Self::Nix => PackageSource::Nix,
        }
    }

    pub const fn default_scope(self) -> PackageScope {
        match self {
            Self::Pacman
            | Self::Apt
            | Self::Dnf5
            | Self::Dnf4
            | Self::Zypper
            | Self::Apk
            | Self::Xbps
            | Self::Snap => PackageScope::System,
            Self::Brew | Self::Nix => PackageScope::Profile,
            Self::Paru | Self::Yay | Self::Flatpak => PackageScope::User,
        }
    }

    pub const fn default_kind(self) -> PackageKind {
        match self {
            Self::Paru | Self::Yay => PackageKind::Aur,
            Self::Flatpak => PackageKind::Flatpak,
            Self::Snap => PackageKind::Snap,
            Self::Brew => PackageKind::BrewFormula,
            Self::Nix => PackageKind::Nix,
            Self::Pacman
            | Self::Apt
            | Self::Dnf5
            | Self::Dnf4
            | Self::Zypper
            | Self::Apk
            | Self::Xbps => PackageKind::System,
        }
    }
}

impl PackageKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Aur => "aur",
            Self::Flatpak => "flatpak",
            Self::Snap => "snap",
            Self::BrewFormula => "brew-formula",
            Self::BrewCask => "brew-cask",
            Self::Nix => "nix",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "aur" => Some(Self::Aur),
            "flatpak" => Some(Self::Flatpak),
            "snap" => Some(Self::Snap),
            "brew-formula" => Some(Self::BrewFormula),
            "brew-cask" => Some(Self::BrewCask),
            "nix" => Some(Self::Nix),
            _ => None,
        }
    }
}

impl PackageScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Profile => "profile",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "user" => Some(Self::User),
            "profile" => Some(Self::Profile),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionAction {
    Install,
    Remove,
    Upgrade,
    Downgrade,
}

impl TransactionAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Remove => "remove",
            Self::Upgrade => "upgrade",
            Self::Downgrade => "downgrade",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BackendId, PackageKind, PackageScope, PackageSource};

    #[test]
    fn every_backend_has_round_trip_metadata_and_provider_scope() {
        for backend in BackendId::ALL {
            assert_eq!(BackendId::parse(backend.as_str()), Some(backend));
            assert_eq!(
                backend.package_source().as_str(),
                match backend {
                    BackendId::Pacman => "pacman",
                    BackendId::Apt => "apt",
                    BackendId::Dnf5 | BackendId::Dnf4 => "dnf",
                    BackendId::Zypper => "zypper",
                    BackendId::Apk => "apk",
                    BackendId::Xbps => "xbps",
                    BackendId::Paru | BackendId::Yay => "aur",
                    BackendId::Flatpak => "flatpak",
                    BackendId::Snap => "snap",
                    BackendId::Brew => "brew",
                    BackendId::Nix => "nix",
                }
            );
            assert_eq!(
                PackageKind::parse(backend.default_kind().as_str()),
                Some(backend.default_kind())
            );
            assert_eq!(
                PackageScope::parse(backend.default_scope().as_str()),
                Some(backend.default_scope())
            );
        }
    }

    #[test]
    fn every_source_has_a_default_backend_scope_and_round_trip_key() {
        for source in PackageSource::ALL {
            assert_eq!(PackageSource::parse(source.as_str()), Some(source));
            assert_eq!(source.backend_for_source().package_source(), source);
            assert_eq!(
                source.default_scope(),
                source.backend_for_source().default_scope()
            );
            assert!(!source.label_key().is_empty());
            assert!(!source.color_code().is_empty());
            assert!(!source.hidden_prefix().is_empty());
        }
        assert_eq!(PackageSource::Dnf.backend_for_source(), BackendId::Dnf5);
        assert_eq!(BackendId::Dnf4.package_source(), PackageSource::Dnf);
        assert_eq!(PackageSource::Aur.backend_for_source(), BackendId::Paru);
        assert_eq!(BackendId::Yay.package_source(), PackageSource::Aur);
        assert_eq!(PackageSource::Snap.default_scope(), PackageScope::System);
    }
}
