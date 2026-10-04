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
            _ => "backend.native",
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
