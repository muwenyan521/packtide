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
