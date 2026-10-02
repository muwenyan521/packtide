use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::plan::CommandPlan;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackendId {
    Pacman,
    Paru,
    Yay,
    Flatpak,
}
impl BackendId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Paru => "paru",
            Self::Yay => "yay",
            Self::Flatpak => "flatpak",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendClass {
    System,
    User,
    Application,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageKind {
    System,
    Aur,
    Flatpak,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    System,
    User,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageId(String);
impl PackageId {
    pub fn new(value: impl Into<String>) -> Result<Self, BackendError> {
        let value = value.into();
        if value.trim().is_empty() {
            Err(BackendError::InvalidPackageId)
        } else {
            Ok(Self(value))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilitySet(u32);
impl CapabilitySet {
    pub const CATALOG: Self = Self(1);
    pub const INSTALL: Self = Self(2);
    pub const REMOVE: Self = Self(4);
    pub const UPGRADE: Self = Self(8);
    pub const DOWNGRADE: Self = Self(16);
    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogStrategy {
    SyncDatabase,
    Cache,
    DirectQuery,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadResult {
    pub backend: BackendId,
    pub packages: Vec<PackageId>,
    pub source: CatalogStrategy,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionPlan {
    pub backend: BackendId,
    pub scope: Scope,
    pub command: CommandPlan,
    pub packages: Vec<PackageId>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendError {
    InvalidPackageId,
    UnsupportedCapability {
        backend: BackendId,
        capability: CapabilitySet,
    },
    InvalidPlan,
}
impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackageId => write!(f, "package id must not be empty"),
            Self::UnsupportedCapability { backend, .. } => write!(
                f,
                "backend {} does not support capability",
                backend.as_str()
            ),
            Self::InvalidPlan => write!(f, "invalid transaction plan"),
        }
    }
}
impl std::error::Error for BackendError {}

pub trait PackageBackend {
    fn id(&self) -> BackendId;
    fn class(&self) -> BackendClass;
    fn kind(&self) -> PackageKind;
    fn scope(&self) -> Scope;
    fn capabilities(&self) -> CapabilitySet;
    fn catalog_strategy(&self) -> CatalogStrategy;
    fn read(&self, packages: Vec<PackageId>) -> Result<ReadResult, BackendError> {
        if !self.capabilities().contains(CapabilitySet::CATALOG) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability: CapabilitySet::CATALOG,
            });
        };
        Ok(ReadResult {
            backend: self.id(),
            packages,
            source: self.catalog_strategy(),
        })
    }
    fn transaction(
        &self,
        action: crate::TransactionAction,
        packages: Vec<PackageId>,
    ) -> Result<TransactionPlan, BackendError>;
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinBackend(pub BackendId);
impl BuiltinBackend {
    pub const fn new(id: BackendId) -> Self {
        Self(id)
    }
}
impl PackageBackend for BuiltinBackend {
    fn id(&self) -> BackendId {
        self.0
    }
    fn class(&self) -> BackendClass {
        match self.0 {
            BackendId::Pacman => BackendClass::System,
            BackendId::Paru | BackendId::Yay => BackendClass::User,
            BackendId::Flatpak => BackendClass::Application,
        }
    }
    fn kind(&self) -> PackageKind {
        match self.0 {
            BackendId::Flatpak => PackageKind::Flatpak,
            BackendId::Paru | BackendId::Yay => PackageKind::Aur,
            BackendId::Pacman => PackageKind::System,
        }
    }
    fn scope(&self) -> Scope {
        if self.0 == BackendId::Pacman {
            Scope::System
        } else {
            Scope::User
        }
    }
    fn capabilities(&self) -> CapabilitySet {
        let base = CapabilitySet::CATALOG
            .union(CapabilitySet::INSTALL)
            .union(CapabilitySet::REMOVE)
            .union(CapabilitySet::UPGRADE);
        if self.0 == BackendId::Flatpak {
            base
        } else {
            base.union(CapabilitySet::DOWNGRADE)
        }
    }
    fn catalog_strategy(&self) -> CatalogStrategy {
        match self.0 {
            BackendId::Flatpak => CatalogStrategy::DirectQuery,
            _ => CatalogStrategy::SyncDatabase,
        }
    }
    fn transaction(
        &self,
        action: crate::TransactionAction,
        packages: Vec<PackageId>,
    ) -> Result<TransactionPlan, BackendError> {
        let cap = match action {
            crate::TransactionAction::Install => CapabilitySet::INSTALL,
            crate::TransactionAction::Remove => CapabilitySet::REMOVE,
            crate::TransactionAction::Upgrade => CapabilitySet::UPGRADE,
            crate::TransactionAction::Downgrade => CapabilitySet::DOWNGRADE,
        };
        if !self.capabilities().contains(cap) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability: cap,
            });
        };
        let mut command =
            CommandPlan::new(PathBuf::from(self.id().as_str())).with_backend(self.id());
        let flag = match (self.0, action) {
            (BackendId::Flatpak, crate::TransactionAction::Install) => "install",
            (BackendId::Flatpak, crate::TransactionAction::Remove) => "uninstall",
            (BackendId::Flatpak, crate::TransactionAction::Upgrade) => "update",
            (BackendId::Flatpak, crate::TransactionAction::Downgrade) => {
                return Err(BackendError::UnsupportedCapability {
                    backend: self.id(),
                    capability: CapabilitySet::DOWNGRADE,
                });
            }
            (_, crate::TransactionAction::Install) => "-S",
            (_, crate::TransactionAction::Remove) => "-Rns",
            (_, crate::TransactionAction::Upgrade) => "-Su",
            (_, crate::TransactionAction::Downgrade) => "-U",
        };
        command.args.push(OsString::from(flag));
        for p in &packages {
            command.args.push(OsString::from(p.as_str()));
        }
        Ok(TransactionPlan {
            backend: self.id(),
            scope: self.scope(),
            command,
            packages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BackendId, BuiltinBackend, PackageBackend, PackageId, Scope};
    use crate::TransactionAction;

    fn ids(values: &[&str]) -> Vec<PackageId> {
        values
            .iter()
            .map(|value| PackageId::new(*value).expect("valid package id"))
            .collect()
    }

    #[test]
    fn arch_backends_keep_typed_transaction_argv_and_scope() {
        let plan = BuiltinBackend::new(BackendId::Paru)
            .transaction(TransactionAction::Remove, ids(&["aur/tool"]))
            .expect("paru remove plan");
        assert_eq!(plan.scope, Scope::User);
        assert_eq!(plan.command.program.to_string_lossy(), "paru");
        assert_eq!(plan.command.args, ["-Rns", "aur/tool"]);
    }

    #[test]
    fn flatpak_uses_application_ids_without_display_labels() {
        let plan = BuiltinBackend::new(BackendId::Flatpak)
            .transaction(TransactionAction::Remove, ids(&["org.example.App"]))
            .expect("flatpak uninstall plan");
        assert_eq!(plan.scope, Scope::User);
        assert_eq!(plan.command.args, ["uninstall", "org.example.App"]);
    }
}
