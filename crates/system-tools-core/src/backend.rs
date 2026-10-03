use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::plan::CommandPlan;
use crate::{CommandPrivilege, TransactionAction};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackendId {
    Pacman,
    Apt,
    Paru,
    Yay,
    Flatpak,
}

impl BackendId {
    pub const ALL: [Self; 5] = [
        Self::Pacman,
        Self::Apt,
        Self::Paru,
        Self::Yay,
        Self::Flatpak,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Paru => "paru",
            Self::Yay => "yay",
            Self::Flatpak => "flatpak",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackendClass {
    Native,
    Optional,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PackageKind {
    System,
    Aur,
    Flatpak,
}

impl PackageKind {
    pub const ALL: [Self; 3] = [Self::System, Self::Aur, Self::Flatpak];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PackageScope {
    System,
    User,
}

impl PackageScope {
    pub const ALL: [Self; 2] = [Self::System, Self::User];
}

pub type Scope = PackageScope;

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

pub type NativePackageKey = PackageId;
pub type NativeKey = NativePackageKey;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageIdentity {
    pub backend: BackendId,
    pub kind: PackageKind,
    pub scope: PackageScope,
    pub native_key: NativePackageKey,
}

impl PackageIdentity {
    pub fn new(
        backend: BackendId,
        kind: PackageKind,
        scope: PackageScope,
        native_key: NativePackageKey,
    ) -> Self {
        Self {
            backend,
            kind,
            scope,
            native_key,
        }
    }

    pub fn key(&self) -> &NativePackageKey {
        &self.native_key
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    pub const CATALOG: Self = Self(1 << 0);
    pub const SEARCH: Self = Self(1 << 1);
    pub const INSTALLED: Self = Self(1 << 2);
    pub const DETAILS: Self = Self(1 << 3);
    pub const UPDATES: Self = Self(1 << 4);
    pub const INSTALL: Self = Self(1 << 5);
    pub const REMOVE: Self = Self(1 << 6);
    pub const UPGRADE: Self = Self(1 << 7);
    pub const DOWNGRADE: Self = Self(1 << 8);
    pub const SYSTEM_UPGRADE: Self = Self(1 << 9);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogStrategy {
    Enumerated,
    QueryRequired,
    SyncDatabase,
    Cache,
    DirectQuery,
}

impl CatalogStrategy {
    pub const fn is_query_required(self) -> bool {
        matches!(self, Self::QueryRequired | Self::DirectQuery)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadOperation {
    Catalog,
    Search { query: String },
    Installed,
    Details { package: PackageId },
    Updates,
}

pub type ReadRequest = ReadOperation;

impl ReadOperation {
    pub const fn capability(&self) -> CapabilitySet {
        match self {
            Self::Catalog => CapabilitySet::CATALOG,
            Self::Search { .. } => CapabilitySet::SEARCH,
            Self::Installed => CapabilitySet::INSTALLED,
            Self::Details { .. } => CapabilitySet::DETAILS,
            Self::Updates => CapabilitySet::UPDATES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadResult {
    pub backend: BackendId,
    pub operation: ReadOperation,
    pub packages: Vec<PackageIdentity>,
    pub source: CatalogStrategy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriteOperation {
    Install { packages: Vec<PackageId> },
    Remove { packages: Vec<PackageId> },
    Upgrade { packages: Vec<PackageId> },
    Downgrade { packages: Vec<PackageId> },
    SystemUpgrade,
}

impl WriteOperation {
    pub fn transaction(action: TransactionAction, packages: Vec<PackageId>) -> Self {
        match action {
            TransactionAction::Install => Self::Install { packages },
            TransactionAction::Remove => Self::Remove { packages },
            TransactionAction::Upgrade => Self::Upgrade { packages },
            TransactionAction::Downgrade => Self::Downgrade { packages },
        }
    }

    pub const fn capability(&self) -> CapabilitySet {
        match self {
            Self::Install { .. } => CapabilitySet::INSTALL,
            Self::Remove { .. } => CapabilitySet::REMOVE,
            Self::Upgrade { .. } => CapabilitySet::UPGRADE,
            Self::Downgrade { .. } => CapabilitySet::DOWNGRADE,
            Self::SystemUpgrade => CapabilitySet::SYSTEM_UPGRADE,
        }
    }

    pub fn packages(&self) -> &[PackageId] {
        match self {
            Self::Install { packages }
            | Self::Remove { packages }
            | Self::Upgrade { packages }
            | Self::Downgrade { packages } => packages,
            Self::SystemUpgrade => &[],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionPlan {
    pub backend: BackendId,
    pub kind: PackageKind,
    pub scope: PackageScope,
    pub operation: WriteOperation,
    pub command: CommandPlan,
    pub packages: Vec<PackageIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendOperation {
    Read(ReadOperation),
    Write(WriteOperation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendResponse {
    Read(ReadResult),
    Write(TransactionPlan),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendError {
    InvalidPackageId,
    UnsupportedCapability {
        backend: BackendId,
        capability: CapabilitySet,
    },
    QueryRequired {
        backend: BackendId,
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
            Self::QueryRequired { backend } => {
                write!(f, "backend {} requires a search query", backend.as_str())
            }
            Self::InvalidPlan => write!(f, "invalid transaction plan"),
        }
    }
}

impl std::error::Error for BackendError {}

pub trait ReadBackend {
    fn read(&self, operation: ReadOperation) -> Result<ReadResult, BackendError>;
}

pub trait WriteBackend {
    fn write(&self, operation: WriteOperation) -> Result<TransactionPlan, BackendError>;
}

pub trait PackageBackend {
    fn id(&self) -> BackendId;
    fn class(&self) -> BackendClass;
    fn kind(&self) -> PackageKind;
    fn scope(&self) -> PackageScope;
    fn capabilities(&self) -> CapabilitySet;
    fn catalog_strategy(&self) -> CatalogStrategy;

    fn identity(&self, native_key: NativePackageKey) -> PackageIdentity {
        PackageIdentity::new(self.id(), self.kind(), self.scope(), native_key)
    }

    fn read(&self, operation: ReadOperation) -> Result<ReadResult, BackendError> {
        let capability = operation.capability();
        if !self.capabilities().contains(capability) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability,
            });
        }
        if matches!(operation, ReadOperation::Catalog)
            && self.catalog_strategy().is_query_required()
        {
            return Err(BackendError::QueryRequired { backend: self.id() });
        }
        let packages = match &operation {
            ReadOperation::Details { package } => vec![self.identity(package.clone())],
            _ => Vec::new(),
        };
        Ok(ReadResult {
            backend: self.id(),
            operation,
            packages,
            source: self.catalog_strategy(),
        })
    }

    fn read_packages(&self, packages: Vec<PackageId>) -> Result<ReadResult, BackendError> {
        if !self.capabilities().contains(CapabilitySet::CATALOG) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability: CapabilitySet::CATALOG,
            });
        }
        Ok(ReadResult {
            backend: self.id(),
            operation: ReadOperation::Catalog,
            packages: packages
                .into_iter()
                .map(|package| self.identity(package))
                .collect(),
            source: self.catalog_strategy(),
        })
    }

    fn catalog(&self) -> Result<ReadResult, BackendError> {
        self.read(ReadOperation::Catalog)
    }

    fn search(&self, query: impl Into<String>) -> Result<ReadResult, BackendError> {
        self.read(ReadOperation::Search {
            query: query.into(),
        })
    }

    fn installed(&self) -> Result<ReadResult, BackendError> {
        self.read(ReadOperation::Installed)
    }

    fn details(&self, package: PackageId) -> Result<ReadResult, BackendError> {
        self.read(ReadOperation::Details { package })
    }

    fn updates(&self) -> Result<ReadResult, BackendError> {
        self.read(ReadOperation::Updates)
    }

    fn write(&self, operation: WriteOperation) -> Result<TransactionPlan, BackendError> {
        let capability = operation.capability();
        if !self.capabilities().contains(capability) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability,
            });
        }
        if !matches!(operation, WriteOperation::SystemUpgrade) && operation.packages().is_empty() {
            return Err(BackendError::InvalidPlan);
        }
        let command = self.command_for(&operation)?;
        let packages = operation
            .packages()
            .iter()
            .cloned()
            .map(|package| self.identity(package))
            .collect();
        Ok(TransactionPlan {
            backend: self.id(),
            kind: self.kind(),
            scope: self.scope(),
            operation,
            command,
            packages,
        })
    }

    fn transaction(
        &self,
        action: TransactionAction,
        packages: Vec<PackageId>,
    ) -> Result<TransactionPlan, BackendError> {
        self.write(WriteOperation::transaction(action, packages))
    }

    fn dispatch(&self, operation: BackendOperation) -> Result<BackendResponse, BackendError> {
        match operation {
            BackendOperation::Read(operation) => self.read(operation).map(BackendResponse::Read),
            BackendOperation::Write(operation) => self.write(operation).map(BackendResponse::Write),
        }
    }

    fn command_for(&self, operation: &WriteOperation) -> Result<CommandPlan, BackendError> {
        let mut command = CommandPlan::new(PathBuf::from(self.id().as_str()))
            .with_backend(self.id())
            .with_locale("C")
            .with_privilege(if self.scope() == PackageScope::System {
                CommandPrivilege::Elevated
            } else {
                CommandPrivilege::User
            });
        let flag = match (self.id(), operation) {
            (BackendId::Flatpak, WriteOperation::Install { .. }) => "install",
            (BackendId::Flatpak, WriteOperation::Remove { .. }) => "uninstall",
            (BackendId::Flatpak, WriteOperation::Upgrade { .. }) => "update",
            (BackendId::Flatpak, WriteOperation::Downgrade { .. })
            | (BackendId::Flatpak, WriteOperation::SystemUpgrade) => {
                return Err(BackendError::UnsupportedCapability {
                    backend: self.id(),
                    capability: CapabilitySet::DOWNGRADE,
                });
            }
            (BackendId::Apt, WriteOperation::Install { .. })
            | (BackendId::Apt, WriteOperation::Upgrade { .. }) => "install",
            (BackendId::Apt, WriteOperation::Remove { .. }) => "remove",
            (_, WriteOperation::Install { .. }) => "-S",
            (_, WriteOperation::Remove { .. }) => "-Rns",
            (_, WriteOperation::Upgrade { .. }) | (_, WriteOperation::SystemUpgrade) => "-Su",
            (_, WriteOperation::Downgrade { .. }) => "-U",
        };
        command.args.push(OsString::from(flag));
        for package in operation.packages() {
            command.args.push(OsString::from(package.as_str()));
        }
        Ok(command)
    }
}

impl<T: PackageBackend + ?Sized> ReadBackend for T {
    fn read(&self, operation: ReadOperation) -> Result<ReadResult, BackendError> {
        PackageBackend::read(self, operation)
    }
}

impl<T: PackageBackend + ?Sized> WriteBackend for T {
    fn write(&self, operation: WriteOperation) -> Result<TransactionPlan, BackendError> {
        PackageBackend::write(self, operation)
    }
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
            BackendId::Pacman | BackendId::Apt => BackendClass::Native,
            BackendId::Paru | BackendId::Yay | BackendId::Flatpak => BackendClass::Optional,
        }
    }

    fn kind(&self) -> PackageKind {
        match self.0 {
            BackendId::Flatpak => PackageKind::Flatpak,
            BackendId::Paru | BackendId::Yay => PackageKind::Aur,
            BackendId::Pacman | BackendId::Apt => PackageKind::System,
        }
    }

    fn scope(&self) -> PackageScope {
        if matches!(self.0, BackendId::Pacman | BackendId::Apt) {
            PackageScope::System
        } else {
            PackageScope::User
        }
    }

    fn capabilities(&self) -> CapabilitySet {
        let read = CapabilitySet::CATALOG
            .union(CapabilitySet::SEARCH)
            .union(CapabilitySet::INSTALLED)
            .union(CapabilitySet::DETAILS)
            .union(CapabilitySet::UPDATES);
        let write = read
            .union(CapabilitySet::INSTALL)
            .union(CapabilitySet::REMOVE)
            .union(CapabilitySet::UPGRADE);
        match self.0 {
            BackendId::Flatpak => write,
            BackendId::Pacman | BackendId::Apt | BackendId::Paru | BackendId::Yay => write
                .union(CapabilitySet::DOWNGRADE)
                .union(CapabilitySet::SYSTEM_UPGRADE),
        }
    }

    fn catalog_strategy(&self) -> CatalogStrategy {
        match self.0 {
            BackendId::Flatpak => CatalogStrategy::QueryRequired,
            BackendId::Pacman | BackendId::Apt | BackendId::Paru | BackendId::Yay => {
                CatalogStrategy::Enumerated
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendClass, BackendError, BackendId, BackendOperation, BackendResponse, BuiltinBackend,
        CapabilitySet, CatalogStrategy, PackageBackend, PackageId, PackageIdentity, PackageKind,
        PackageScope, ReadOperation, Scope, WriteOperation,
    };
    use crate::{CommandPrivilege, TransactionAction};

    fn ids(values: &[&str]) -> Vec<PackageId> {
        values
            .iter()
            .map(|value| PackageId::new(*value).expect("valid package id"))
            .collect()
    }

    #[test]
    fn all_declared_backend_and_kind_variants_have_explicit_identity_metadata() {
        for backend in BackendId::ALL {
            let builtin = BuiltinBackend::new(backend);
            assert!(matches!(
                builtin.class(),
                BackendClass::Native | BackendClass::Optional
            ));
            assert!(PackageKind::ALL.contains(&builtin.kind()));
            assert!(PackageScope::ALL.contains(&builtin.scope()));
        }
        assert_eq!(Scope::User, PackageScope::User);
    }

    #[test]
    fn same_native_name_different_backend_or_scope_is_not_equal() {
        let pacman = PackageIdentity::new(
            BackendId::Pacman,
            PackageKind::System,
            PackageScope::System,
            PackageId::new("same-name").unwrap(),
        );
        let aur = PackageIdentity::new(
            BackendId::Paru,
            PackageKind::Aur,
            PackageScope::User,
            PackageId::new("same-name").unwrap(),
        );
        assert_ne!(pacman, aur);
        assert_eq!(pacman.key().as_str(), "same-name");
    }

    #[test]
    fn arch_backends_keep_typed_transaction_argv_and_scope() {
        let plan = BuiltinBackend::new(BackendId::Paru)
            .transaction(TransactionAction::Remove, ids(&["aur/tool"]))
            .expect("paru remove plan");
        assert_eq!(plan.scope, PackageScope::User);
        assert_eq!(plan.kind, PackageKind::Aur);
        assert_eq!(plan.command.program.to_string_lossy(), "paru");
        assert_eq!(plan.command.args, ["-Rns", "aur/tool"]);
        assert_eq!(
            plan.command.locale.as_deref(),
            Some(std::ffi::OsStr::new("C"))
        );
        assert_eq!(plan.command.privilege, CommandPrivilege::User);
        assert_eq!(plan.packages[0].native_key.as_str(), "aur/tool");
    }

    #[test]
    fn flatpak_uses_application_ids_without_display_labels() {
        let plan = BuiltinBackend::new(BackendId::Flatpak)
            .transaction(TransactionAction::Remove, ids(&["org.example.App"]))
            .expect("flatpak uninstall plan");
        assert_eq!(plan.scope, PackageScope::User);
        assert_eq!(plan.command.args, ["uninstall", "org.example.App"]);
        assert_eq!(plan.packages[0].native_key.as_str(), "org.example.App");
    }

    #[test]
    fn read_contract_has_distinct_catalog_search_details_installed_and_updates() {
        let backend = BuiltinBackend::new(BackendId::Pacman);
        assert_eq!(backend.catalog().unwrap().operation, ReadOperation::Catalog);
        assert_eq!(
            backend.search("bash").unwrap().operation,
            ReadOperation::Search {
                query: "bash".to_owned()
            }
        );
        assert_eq!(
            backend.installed().unwrap().operation,
            ReadOperation::Installed
        );
        assert!(matches!(
            backend
                .details(PackageId::new("bash").unwrap())
                .unwrap()
                .operation,
            ReadOperation::Details { .. }
        ));
        assert_eq!(backend.updates().unwrap().operation, ReadOperation::Updates);
    }

    #[test]
    fn query_required_catalog_and_unsupported_write_fail_before_execution() {
        let flatpak = BuiltinBackend::new(BackendId::Flatpak);
        assert_eq!(
            flatpak.catalog(),
            Err(BackendError::QueryRequired {
                backend: BackendId::Flatpak
            })
        );
        assert_eq!(
            flatpak.write(WriteOperation::Downgrade {
                packages: ids(&["org.example.App"])
            }),
            Err(BackendError::UnsupportedCapability {
                backend: BackendId::Flatpak,
                capability: CapabilitySet::DOWNGRADE
            })
        );
    }

    #[test]
    fn dispatch_keeps_read_and_write_response_types_separate() {
        let backend = BuiltinBackend::new(BackendId::Pacman);
        let read = backend
            .dispatch(BackendOperation::Read(ReadOperation::Updates))
            .unwrap();
        assert!(matches!(read, BackendResponse::Read(_)));
        let write = backend
            .dispatch(BackendOperation::Write(WriteOperation::Install {
                packages: ids(&["core/bash"]),
            }))
            .unwrap();
        assert!(matches!(write, BackendResponse::Write(_)));
    }

    #[test]
    fn empty_package_transaction_is_rejected() {
        let error = BuiltinBackend::new(BackendId::Pacman)
            .write(WriteOperation::Install {
                packages: Vec::new(),
            })
            .unwrap_err();
        assert_eq!(error, BackendError::InvalidPlan);
    }

    #[test]
    fn catalog_strategy_declares_enumerated_or_query_required() {
        assert_eq!(
            BuiltinBackend::new(BackendId::Pacman).catalog_strategy(),
            CatalogStrategy::Enumerated
        );
        assert_eq!(
            BuiltinBackend::new(BackendId::Flatpak).catalog_strategy(),
            CatalogStrategy::QueryRequired
        );
    }
}
