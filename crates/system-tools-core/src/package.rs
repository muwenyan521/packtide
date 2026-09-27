#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PackageSource {
    Pacman,
    Aur,
    Flatpak,
}

impl PackageSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Aur => "aur",
            Self::Flatpak => "flatpak",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pacman" => Some(Self::Pacman),
            "aur" => Some(Self::Aur),
            "flatpak" => Some(Self::Flatpak),
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
