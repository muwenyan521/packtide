use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;

use crate::ExecutableResolver;
use crate::backends::apt::AptBackend;
use crate::backends::dnf::{DnfBackend, DnfGeneration};
use crate::backends::{apk::ApkBackend, xbps::XbpsBackend, zypper::ZypperBackend};
use crate::plan::CommandPlan;
use crate::{CommandPrivilege, TransactionAction};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackendId {
    Pacman,
    Apt,
    Dnf5,
    Dnf4,
    Zypper,
    Apk,
    Xbps,
    Paru,
    Yay,
    Flatpak,
    Snap,
    Brew,
    Nix,
}

impl BackendId {
    pub const ALL: [Self; 13] = [
        Self::Pacman,
        Self::Apt,
        Self::Dnf5,
        Self::Dnf4,
        Self::Zypper,
        Self::Apk,
        Self::Xbps,
        Self::Paru,
        Self::Yay,
        Self::Flatpak,
        Self::Snap,
        Self::Brew,
        Self::Nix,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Dnf5 => "dnf5",
            Self::Dnf4 => "dnf4",
            Self::Zypper => "zypper",
            Self::Apk => "apk",
            Self::Xbps => "xbps",
            Self::Paru => "paru",
            Self::Yay => "yay",
            Self::Flatpak => "flatpak",
            Self::Snap => "snap",
            Self::Brew => "brew",
            Self::Nix => "nix",
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
    Snap,
    BrewFormula,
    BrewCask,
    Nix,
}

impl PackageKind {
    pub const ALL: [Self; 7] = [
        Self::System,
        Self::Aur,
        Self::Flatpak,
        Self::Snap,
        Self::BrewFormula,
        Self::BrewCask,
        Self::Nix,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PackageScope {
    System,
    User,
    Profile,
}

impl PackageScope {
    pub const ALL: [Self; 3] = [Self::System, Self::User, Self::Profile];
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativePackageKey(String);

impl NativePackageKey {
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

impl From<PackageId> for NativePackageKey {
    fn from(value: PackageId) -> Self {
        Self(value.0)
    }
}

impl fmt::Display for NativePackageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub type NativeKey = NativePackageKey;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageIdentity {
    pub backend: BackendId,
    pub kind: PackageKind,
    pub scope: PackageScope,
    pub native_key: NativePackageKey,
    pub origin: Option<String>,
    pub display_name: Option<String>,
}

impl PackageIdentity {
    pub fn new(
        backend: BackendId,
        kind: PackageKind,
        scope: PackageScope,
        native_key: impl Into<NativePackageKey>,
    ) -> Self {
        Self {
            backend,
            kind,
            scope,
            native_key: native_key.into(),
            origin: None,
            display_name: None,
        }
    }

    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
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
    pub const REFRESH_CATALOG: Self = Self(1 << 10);

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
    RefreshCatalog,
    Search {
        query: String,
    },
    Installed,
    Details {
        package: PackageId,
        scope: PackageScope,
    },
    Updates,
}

pub type ReadRequest = ReadOperation;

impl ReadOperation {
    pub const fn capability(&self) -> CapabilitySet {
        match self {
            Self::Catalog => CapabilitySet::CATALOG,
            Self::RefreshCatalog => CapabilitySet::REFRESH_CATALOG,
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
    pub details: Option<ReadDetails>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadDetails {
    pub stdout: String,
    pub stderr: String,
    pub status: ExitStatus,
    pub success: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriteOperation {
    Install { packages: Vec<PackageIdentity> },
    Remove { packages: Vec<PackageIdentity> },
    Upgrade { packages: Vec<PackageIdentity> },
    Downgrade { packages: Vec<PackageIdentity> },
    SystemUpgrade,
}

impl WriteOperation {
    pub fn transaction(action: TransactionAction, packages: Vec<PackageIdentity>) -> Self {
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

    pub fn packages(&self) -> &[PackageIdentity] {
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
    CommandUnavailable {
        backend: BackendId,
        operation: &'static str,
        command: String,
    },
    CommandFailed {
        backend: BackendId,
        operation: &'static str,
        message: String,
    },
    IdentityMismatch {
        expected_backend: BackendId,
        actual_backend: BackendId,
        expected_kind: PackageKind,
        actual_kind: PackageKind,
        expected_scope: PackageScope,
        actual_scope: PackageScope,
    },
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
            Self::CommandUnavailable {
                backend,
                operation,
                command,
            } => write!(
                f,
                "backend {} cannot {operation}: required command '{command}' is unavailable",
                backend.as_str()
            ),
            Self::CommandFailed {
                backend,
                operation,
                message,
            } => write!(
                f,
                "backend {} failed to {operation}: {message}",
                backend.as_str()
            ),
            Self::IdentityMismatch {
                expected_backend,
                actual_backend,
                expected_kind,
                actual_kind,
                expected_scope,
                actual_scope,
            } => write!(
                f,
                "package identity does not belong to backend {}: got {} ({actual_kind:?}, {actual_scope:?}), expected ({expected_kind:?}, {expected_scope:?})",
                expected_backend.as_str(),
                actual_backend.as_str(),
            ),
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
        if matches!(
            operation,
            ReadOperation::Catalog | ReadOperation::RefreshCatalog
        ) && self.catalog_strategy().is_query_required()
        {
            return Err(BackendError::QueryRequired { backend: self.id() });
        }
        let packages = match &operation {
            ReadOperation::Details { package, scope } => {
                vec![PackageIdentity::new(
                    self.id(),
                    self.kind(),
                    *scope,
                    package.clone(),
                )]
            }
            _ => Vec::new(),
        };
        Ok(ReadResult {
            backend: self.id(),
            operation,
            packages,
            source: self.catalog_strategy(),
            details: None,
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
                .map(|package| self.identity(package.into()))
                .collect(),
            source: self.catalog_strategy(),
            details: None,
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
        self.read(ReadOperation::Details {
            package,
            scope: self.scope(),
        })
    }

    fn updates(&self) -> Result<ReadResult, BackendError> {
        self.read(ReadOperation::Updates)
    }

    fn write(&self, operation: WriteOperation) -> Result<TransactionPlan, BackendError> {
        let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
        self.write_with_resolver(operation, &resolver)
    }

    fn write_with_resolver(
        &self,
        operation: WriteOperation,
        resolver: &ExecutableResolver,
    ) -> Result<TransactionPlan, BackendError> {
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
        let command = self.command_for_with_resolver(&operation, resolver)?;
        let packages = operation.packages().to_vec();
        let scope = packages
            .first()
            .map_or(self.scope(), |package| package.scope);
        Ok(TransactionPlan {
            backend: self.id(),
            kind: self.kind(),
            scope,
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
        let identities = packages
            .into_iter()
            .map(|package| self.identity(package.into()))
            .collect();
        self.write(WriteOperation::transaction(action, identities))
    }

    fn transaction_with_resolver(
        &self,
        action: TransactionAction,
        packages: Vec<PackageId>,
        resolver: &ExecutableResolver,
    ) -> Result<TransactionPlan, BackendError> {
        let identities = packages
            .into_iter()
            .map(|package| self.identity(package.into()))
            .collect();
        self.write_with_resolver(WriteOperation::transaction(action, identities), resolver)
    }

    fn dispatch(&self, operation: BackendOperation) -> Result<BackendResponse, BackendError> {
        match operation {
            BackendOperation::Read(operation) => self.read(operation).map(BackendResponse::Read),
            BackendOperation::Write(operation) => self.write(operation).map(BackendResponse::Write),
        }
    }

    fn command_for(&self, operation: &WriteOperation) -> Result<CommandPlan, BackendError> {
        let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
        self.command_for_with_resolver(operation, &resolver)
    }

    fn command_for_with_resolver(
        &self,
        operation: &WriteOperation,
        resolver: &ExecutableResolver,
    ) -> Result<CommandPlan, BackendError> {
        let capability = operation.capability();
        if !self.capabilities().contains(capability) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability,
            });
        }
        if matches!(
            self.id(),
            BackendId::Apt
                | BackendId::Dnf5
                | BackendId::Dnf4
                | BackendId::Snap
                | BackendId::Brew
                | BackendId::Nix
                | BackendId::Zypper
                | BackendId::Apk
                | BackendId::Xbps
        ) {
            if let Some(package) = operation.packages().iter().find(|package| {
                package.backend != self.id()
                    || package.kind != self.kind()
                    || package.scope != self.scope()
            }) {
                return Err(BackendError::IdentityMismatch {
                    expected_backend: self.id(),
                    actual_backend: package.backend,
                    expected_kind: self.kind(),
                    actual_kind: package.kind,
                    expected_scope: self.scope(),
                    actual_scope: package.scope,
                });
            }
            let executable = match self.id() {
                BackendId::Apt => "apt-get",
                BackendId::Dnf5 => "dnf5",
                BackendId::Dnf4 => "dnf",
                BackendId::Xbps => "xbps-install",
                _ => self.id().as_str(),
            };
            let program = resolver.resolve(OsStr::new(executable)).ok_or_else(|| {
                BackendError::CommandUnavailable {
                    backend: self.id(),
                    operation: "write transaction",
                    command: executable.to_owned(),
                }
            })?;
            let result = match self.id() {
                BackendId::Apt => AptBackend::from_paths(&program, &program, &program)
                    .transaction(operation.clone())
                    .map_err(|error| error.to_string()),
                BackendId::Dnf5 | BackendId::Dnf4 => {
                    let generation = if self.id() == BackendId::Dnf5 {
                        DnfGeneration::Dnf5
                    } else {
                        DnfGeneration::Dnf4
                    };
                    DnfBackend::from_paths(generation, program)
                        .transaction(operation.clone())
                        .map_err(|error| error.to_string())
                }
                BackendId::Snap => crate::backends::snap::SnapBackend::from_paths(program)
                    .transaction(operation.clone())
                    .map_err(|error| error.to_string()),
                BackendId::Brew => crate::backends::brew::BrewBackend::from_paths(program)
                    .transaction(operation.clone())
                    .map_err(|error| error.to_string()),
                BackendId::Nix => crate::backends::nix::NixBackend::from_paths(program)
                    .transaction(operation.clone())
                    .map_err(|error| error.to_string()),
                BackendId::Zypper => ZypperBackend::from_paths(program)
                    .transaction(operation.clone())
                    .map_err(|error| error.to_string()),
                BackendId::Apk => ApkBackend::from_paths(program)
                    .transaction(operation.clone())
                    .map_err(|error| error.to_string()),
                BackendId::Xbps => {
                    let query = resolver.resolve(OsStr::new("xbps-query")).ok_or_else(|| {
                        BackendError::CommandUnavailable {
                            backend: self.id(),
                            operation: "write transaction",
                            command: "xbps-query".to_owned(),
                        }
                    })?;
                    let remove = resolver.resolve(OsStr::new("xbps-remove")).ok_or_else(|| {
                        BackendError::CommandUnavailable {
                            backend: self.id(),
                            operation: "write transaction",
                            command: "xbps-remove".to_owned(),
                        }
                    })?;
                    XbpsBackend::from_paths(query, program, remove)
                        .transaction(operation.clone())
                        .map_err(|error| error.to_string())
                }
                _ => unreachable!(),
            };
            return result.map(|plan| plan.command).map_err(|message| {
                BackendError::CommandFailed {
                    backend: self.id(),
                    operation: "write transaction",
                    message,
                }
            });
        }
        let expected_scope = operation
            .packages()
            .first()
            .filter(|_| self.id() == BackendId::Flatpak)
            .map_or(self.scope(), |package| package.scope);
        if let Some(package) = operation.packages().iter().find(|package| {
            package.backend != self.id()
                || package.kind != self.kind()
                || package.scope != expected_scope
        }) {
            return Err(BackendError::IdentityMismatch {
                expected_backend: self.id(),
                actual_backend: package.backend,
                expected_kind: self.kind(),
                actual_kind: package.kind,
                expected_scope,
                actual_scope: package.scope,
            });
        }
        let program = resolver
            .resolve(OsStr::new(self.id().as_str()))
            .ok_or_else(|| BackendError::CommandUnavailable {
                backend: self.id(),
                operation: "write transaction",
                command: self.id().as_str().to_owned(),
            })?;
        let mut command = CommandPlan::new(program)
            .with_backend(self.id())
            .with_env_remove("LD_PRELOAD")
            .with_env_remove("LD_LIBRARY_PATH")
            .with_locale("C")
            .with_privilege(if expected_scope == PackageScope::System {
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
                    capability: operation.capability(),
                });
            }
            (BackendId::Xbps, WriteOperation::Install { .. })
            | (BackendId::Xbps, WriteOperation::Remove { .. }) => "-y",
            (_, WriteOperation::Install { .. }) => "-S",
            (_, WriteOperation::Remove { .. }) => "-Rns",
            (_, WriteOperation::Upgrade { .. }) | (_, WriteOperation::SystemUpgrade) => "-Su",
            (_, WriteOperation::Downgrade { .. }) => "-U",
        };
        command.args.push(OsString::from(flag));
        if self.id() == BackendId::Flatpak && expected_scope == PackageScope::System {
            command.args.push(OsString::from("--system"));
        }
        for package in operation.packages() {
            command
                .args
                .push(OsString::from(package.native_key.as_str()));
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
            BackendId::Pacman
            | BackendId::Apt
            | BackendId::Dnf5
            | BackendId::Dnf4
            | BackendId::Zypper
            | BackendId::Apk
            | BackendId::Xbps => BackendClass::Native,
            BackendId::Paru
            | BackendId::Yay
            | BackendId::Flatpak
            | BackendId::Snap
            | BackendId::Brew
            | BackendId::Nix => BackendClass::Optional,
        }
    }

    fn kind(&self) -> PackageKind {
        self.0.default_kind()
    }

    fn scope(&self) -> PackageScope {
        self.0.default_scope()
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
        let install_remove_system_upgrade = read
            .union(CapabilitySet::INSTALL)
            .union(CapabilitySet::REMOVE)
            .union(CapabilitySet::SYSTEM_UPGRADE);
        let refresh = read.union(CapabilitySet::REFRESH_CATALOG);
        match self.0 {
            BackendId::Flatpak => write.union(CapabilitySet::REFRESH_CATALOG),
            BackendId::Brew | BackendId::Nix => write,
            BackendId::Snap => write.union(CapabilitySet::SYSTEM_UPGRADE),
            BackendId::Pacman | BackendId::Paru | BackendId::Yay => write
                .union(CapabilitySet::DOWNGRADE)
                .union(CapabilitySet::SYSTEM_UPGRADE),
            BackendId::Apt => write
                .union(CapabilitySet::SYSTEM_UPGRADE)
                .union(CapabilitySet::REFRESH_CATALOG),
            BackendId::Dnf5 | BackendId::Dnf4 => write.union(CapabilitySet::SYSTEM_UPGRADE),
            BackendId::Zypper => refresh
                .union(CapabilitySet::INSTALL)
                .union(CapabilitySet::REMOVE)
                .union(CapabilitySet::UPGRADE)
                .union(CapabilitySet::SYSTEM_UPGRADE),
            BackendId::Apk | BackendId::Xbps => {
                install_remove_system_upgrade.union(CapabilitySet::REFRESH_CATALOG)
            }
        }
    }

    fn catalog_strategy(&self) -> CatalogStrategy {
        match self.0 {
            BackendId::Snap | BackendId::Nix => CatalogStrategy::DirectQuery,
            BackendId::Pacman
            | BackendId::Apt
            | BackendId::Dnf5
            | BackendId::Dnf4
            | BackendId::Zypper
            | BackendId::Apk
            | BackendId::Xbps
            | BackendId::Paru
            | BackendId::Yay
            | BackendId::Flatpak
            | BackendId::Brew => CatalogStrategy::Enumerated,
        }
    }

    fn read(&self, operation: ReadOperation) -> Result<ReadResult, BackendError> {
        let capability = operation.capability();
        if !self.capabilities().contains(capability) {
            return Err(BackendError::UnsupportedCapability {
                backend: self.id(),
                capability,
            });
        }
        if matches!(
            operation,
            ReadOperation::Catalog | ReadOperation::RefreshCatalog
        ) && self.catalog_strategy().is_query_required()
        {
            return Err(BackendError::QueryRequired { backend: self.id() });
        }

        let (packages, source, details) = match self.0 {
            BackendId::Pacman => read_pacman(operation.clone(), self.id())?,
            BackendId::Paru | BackendId::Yay => read_aur(operation.clone(), self.id())?,
            BackendId::Flatpak => read_flatpak(operation.clone(), self.id())?,
            BackendId::Apt => read_apt(operation.clone(), self.id())?,
            BackendId::Dnf5 | BackendId::Dnf4 => read_dnf(operation.clone(), self.id())?,
            BackendId::Snap => read_snap(operation.clone())?,
            BackendId::Brew => read_brew(operation.clone())?,
            BackendId::Nix => read_nix(operation.clone())?,
            BackendId::Zypper => read_zypper(operation.clone())?,
            BackendId::Apk => read_apk(operation.clone())?,
            BackendId::Xbps => read_xbps(operation.clone())?,
        };
        Ok(ReadResult {
            backend: self.id(),
            operation,
            packages,
            source,
            details,
        })
    }
}

fn read_pacman(
    operation: ReadOperation,
    backend: BackendId,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    match operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog | ReadOperation::Search { .. } => {
            let output =
                run_backend_command(backend, "enumerate packages", &["--color=never", "-Sl"])?;
            if !output.status.success() {
                return Err(command_failed(backend, "enumerate packages", &output));
            }
            let query = match operation {
                ReadOperation::Search { query } => Some(query),
                _ => None,
            };
            let packages = output
                .stdout
                .lines()
                .filter_map(parse_pacman_sync_identity)
                .filter(|identity| {
                    query
                        .as_deref()
                        .is_none_or(|query| identity.native_key.as_str().contains(query))
                })
                .collect();
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
        ReadOperation::Installed => {
            let output =
                run_backend_command(backend, "list installed packages", &["--color=never", "-Q"])?;
            if !output.status.success() {
                return Err(command_failed(backend, "list installed packages", &output));
            }
            let packages = output
                .stdout
                .lines()
                .filter_map(parse_pacman_installed_identity)
                .collect();
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
        ReadOperation::Details { package, .. } => {
            let key = package.as_str();
            let qi = key.strip_prefix("detail-qi:").is_some();
            let key = key.strip_prefix("detail-qi:").unwrap_or(key);
            let output = run_backend_command(
                backend,
                "read package details",
                &["--color=always", if qi { "-Qi" } else { "-Si" }, key],
            )?;
            Ok((
                vec![identity_for(
                    backend,
                    PackageKind::System,
                    PackageScope::System,
                    key.to_owned(),
                )?],
                CatalogStrategy::Enumerated,
                Some(ReadDetails {
                    stdout: output.stdout,
                    stderr: output.stderr,
                    status: output.status,
                    success: output.status.success(),
                }),
            ))
        }
        ReadOperation::Updates => {
            let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
            let Some(path) = resolver.resolve(OsStr::new("checkupdates")) else {
                return Err(BackendError::CommandUnavailable {
                    backend,
                    operation: "list package updates",
                    command: "checkupdates".to_owned(),
                });
            };
            let output = crate::run_capture_path(&path, &[] as &[&str], true).map_err(|error| {
                BackendError::CommandFailed {
                    backend,
                    operation: "list package updates",
                    message: error.to_string(),
                }
            })?;
            if !output.status.success() && output.status.code() != Some(2) {
                return Err(command_failed(backend, "list package updates", &output));
            }
            let packages = output
                .stdout
                .lines()
                .filter_map(|line| parse_update_identity(line).map(|name| (name, line)))
                .map(|(name, line)| {
                    identity_for(backend, PackageKind::System, PackageScope::System, name)
                        .map(|identity| identity.with_display_name(line))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
    }
}

fn read_aur(
    operation: ReadOperation,
    backend: BackendId,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let helper = backend.as_str();
    match operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog | ReadOperation::Search { .. } => {
            let names = aur_names(backend)?;
            let query = match operation {
                ReadOperation::Search { query } => Some(query),
                _ => None,
            };
            let packages = names
                .lines()
                .map(str::trim)
                .filter(|name| valid_package_token(name))
                .filter(|name| query.as_deref().is_none_or(|query| name.contains(query)))
                .map(|name| {
                    identity_for(
                        backend,
                        PackageKind::Aur,
                        PackageScope::User,
                        name.to_owned(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
        ReadOperation::Installed => {
            let output = run_backend_command_for(
                backend,
                "pacman",
                "list foreign installed packages",
                &["--color=never", "-Qm"],
            )?;
            if !output.status.success() {
                return Err(command_failed(
                    backend,
                    "list foreign installed packages",
                    &output,
                ));
            }
            let packages = output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|name| valid_package_token(name))
                .map(|name| {
                    identity_for(
                        backend,
                        PackageKind::Aur,
                        PackageScope::User,
                        name.to_owned(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
        ReadOperation::Details { package, .. } => {
            let qi = package.as_str().strip_prefix("detail-qi:").is_some();
            let package = package
                .as_str()
                .strip_prefix("detail-qi:")
                .unwrap_or(package.as_str());
            let package = package.strip_prefix("aur/").unwrap_or(package);
            let output = run_backend_command_for(
                backend,
                helper,
                "read AUR package details",
                &["--color=always", if qi { "-Qi" } else { "-Si" }, package],
            )?;
            Ok((
                vec![identity_for(
                    backend,
                    PackageKind::Aur,
                    PackageScope::User,
                    package.to_owned(),
                )?],
                CatalogStrategy::Enumerated,
                Some(ReadDetails {
                    stdout: output.stdout,
                    stderr: output.stderr,
                    status: output.status,
                    success: output.status.success(),
                }),
            ))
        }
        ReadOperation::Updates => {
            let output = run_backend_command_for(backend, helper, "list AUR updates", &["-Qua"])?;
            if !output.status.success() {
                return Err(command_failed(backend, "list AUR updates", &output));
            }
            let packages = output
                .stdout
                .lines()
                .filter_map(|line| parse_update_identity(line).map(|name| (name, line)))
                .map(|(name, line)| {
                    identity_for(backend, PackageKind::Aur, PackageScope::User, name)
                        .map(|identity| identity.with_display_name(line))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
    }
}

fn execute_plan(
    plan: &CommandPlan,
    backend: BackendId,
    operation: &'static str,
) -> Result<crate::Output, BackendError> {
    crate::command::run_capture_plan(plan).map_err(|e| BackendError::CommandFailed {
        backend,
        operation,
        message: e.to_string(),
    })
}

fn read_apt(
    operation: ReadOperation,
    backend: BackendId,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let b = AptBackend::from_path(std::env::var_os("PATH").as_deref()).map_err(|e| {
        BackendError::CommandUnavailable {
            backend,
            operation: "read packages",
            command: e.to_string(),
        }
    })?;
    let (plan, parser): (CommandPlan, u8) = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog | ReadOperation::Search { .. } => {
            (b.catalog_plan(), 0)
        }
        ReadOperation::Installed => (b.installed_plan(), 1),
        ReadOperation::Details { package, .. } => (
            b.details_plan(package)
                .map_err(|e| BackendError::CommandFailed {
                    backend,
                    operation: "read package details",
                    message: e.to_string(),
                })?,
            2,
        ),
        ReadOperation::Updates => (b.updates_plan(), 3),
    };
    if matches!(operation, ReadOperation::RefreshCatalog) {
        let refresh = execute_plan(&b.update_catalog_plan(), backend, "refresh package catalog")?;
        if !refresh.status.success() {
            return Err(command_failed(backend, "refresh package catalog", &refresh));
        }
    }
    let out = execute_plan(&plan, backend, "read packages")?;
    if !out.status.success() && parser != 2 {
        return Err(command_failed(backend, "read packages", &out));
    }
    if parser == 2 {
        let key = match &operation {
            ReadOperation::Details { package, .. } => package.as_str().to_owned(),
            _ => unreachable!(),
        };
        let id = identity_for(backend, PackageKind::System, PackageScope::System, key)?;
        return Ok((
            vec![id],
            CatalogStrategy::Enumerated,
            Some(ReadDetails {
                stdout: out.stdout,
                stderr: out.stderr,
                status: out.status,
                success: out.status.success(),
            }),
        ));
    }
    let mut packages: Vec<PackageIdentity> = match parser {
        0 => crate::backends::apt::parse_deb822(&out.stdout)
            .map_err(|e| BackendError::CommandFailed {
                backend,
                operation: "parse package catalog",
                message: e.to_string(),
            })?
            .into_iter()
            .filter_map(|p| {
                identity_for(backend, PackageKind::System, PackageScope::System, p.name).ok()
            })
            .collect(),
        1 => crate::backends::apt::parse_dpkg_query(&out.stdout)
            .map_err(|e| BackendError::CommandFailed {
                backend,
                operation: "parse installed packages",
                message: e.to_string(),
            })?
            .into_iter()
            .filter(|p| p.installed)
            .filter_map(|p| {
                identity_for(backend, PackageKind::System, PackageScope::System, p.name).ok()
            })
            .collect(),
        _ => crate::backends::apt::parse_simulation(&out.stdout)
            .map_err(|e| BackendError::CommandFailed {
                backend,
                operation: "parse package updates",
                message: e.to_string(),
            })?
            .into_iter()
            .filter_map(|p| {
                let display = format!(
                    "{} {} -> {}",
                    p.name,
                    p.current.as_deref().unwrap_or("?"),
                    p.candidate
                );
                identity_for(backend, PackageKind::System, PackageScope::System, p.name)
                    .ok()
                    .map(|identity| identity.with_display_name(display))
            })
            .collect(),
    };
    if let ReadOperation::Search { query } = operation {
        packages.retain(|p| p.native_key.as_str().contains(&query));
    }
    Ok((packages, CatalogStrategy::Enumerated, None))
}

fn read_dnf(
    operation: ReadOperation,
    backend: BackendId,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    read_dnf_with_resolver(operation, backend, &resolver)
}

fn read_dnf_with_resolver(
    operation: ReadOperation,
    backend: BackendId,
    resolver: &ExecutableResolver,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let generation = if backend == BackendId::Dnf5 {
        DnfGeneration::Dnf5
    } else {
        DnfGeneration::Dnf4
    };
    let executable = match generation {
        DnfGeneration::Dnf5 => "dnf5",
        DnfGeneration::Dnf4 => "dnf",
    };
    let program = resolver.resolve(OsStr::new(executable)).ok_or_else(|| {
        BackendError::CommandUnavailable {
            backend,
            operation: "read packages",
            command: executable.to_owned(),
        }
    })?;
    let b = DnfBackend::from_paths(generation, program);
    let (plan, parser) = match &operation {
        ReadOperation::RefreshCatalog => {
            return Err(BackendError::UnsupportedCapability {
                backend,
                capability: CapabilitySet::REFRESH_CATALOG,
            });
        }
        ReadOperation::Catalog | ReadOperation::Search { .. } => (b.list_plan(), false),
        ReadOperation::Installed => (b.installed_plan(), true),
        ReadOperation::Updates => (b.updates_plan(), false),
        ReadOperation::Details { package, .. } => (
            b.details_plan(package)
                .map_err(|e| BackendError::CommandFailed {
                    backend,
                    operation: "read package details",
                    message: e.to_string(),
                })?,
            false,
        ),
    };
    let out = execute_plan(&plan, backend, "read packages")?;
    if !out.status.success()
        && !(matches!(operation, ReadOperation::Catalog | ReadOperation::Updates)
            && out.status.code() == Some(100))
        && !matches!(operation, ReadOperation::Details { .. })
    {
        return Err(command_failed(backend, "read packages", &out));
    }
    if matches!(operation, ReadOperation::Details { .. }) {
        let key = match operation {
            ReadOperation::Details { package, .. } => package.as_str().to_owned(),
            _ => unreachable!(),
        };
        return Ok((
            vec![identity_for(
                backend,
                PackageKind::System,
                PackageScope::System,
                key,
            )?],
            CatalogStrategy::Enumerated,
            Some(ReadDetails {
                stdout: out.stdout,
                stderr: out.stderr,
                status: out.status,
                success: out.status.success(),
            }),
        ));
    }
    let parsed = crate::backends::dnf::parse_dnf4_table(&out.stdout).map_err(|e| {
        BackendError::CommandFailed {
            backend,
            operation: "parse DNF catalog",
            message: e.to_string(),
        }
    })?;
    let mut packages: Vec<_> = parsed
        .into_iter()
        .filter(|p| !parser || p.installed)
        .filter_map(|p| {
            let display = format!("{} {}", p.name, p.version);
            identity_for(backend, PackageKind::System, PackageScope::System, p.name)
                .ok()
                .map(|identity| {
                    if matches!(operation, ReadOperation::Updates) {
                        identity.with_display_name(display)
                    } else {
                        identity
                    }
                })
        })
        .collect();
    if let ReadOperation::Search { query } = operation {
        packages.retain(|p| p.native_key.as_str().contains(&query));
    }
    Ok((packages, CatalogStrategy::Enumerated, None))
}

fn optional_read_error(backend: BackendId, error: impl fmt::Display) -> BackendError {
    BackendError::CommandFailed {
        backend,
        operation: "read packages",
        message: error.to_string(),
    }
}

fn read_snap(
    operation: ReadOperation,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    use crate::backends::snap::{SnapBackend, parse_info, parse_list, parse_search, parse_updates};
    let backend = BackendId::Snap;
    let provider =
        SnapBackend::from_path(std::env::var_os("PATH").as_deref()).map_err(|error| {
            BackendError::CommandUnavailable {
                backend,
                operation: "read packages",
                command: error.to_string(),
            }
        })?;
    let plan = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog => {
            return Err(BackendError::QueryRequired { backend });
        }
        ReadOperation::Search { query } => provider.search_plan(query),
        ReadOperation::Installed => provider.installed_plan(),
        ReadOperation::Updates => provider.updates_plan(),
        ReadOperation::Details { package, scope } => {
            if *scope != PackageScope::System {
                return Err(optional_read_error(backend, "snap requires system scope"));
            }
            provider
                .details_plan(package)
                .map_err(|error| optional_read_error(backend, error))?
        }
    };
    let output = execute_plan(&plan, backend, "read packages")?;
    if !output.status.success() {
        return Err(command_failed(backend, "read packages", &output));
    }
    let packages = match &operation {
        ReadOperation::Updates => parse_updates(&output.stdout)
            .map_err(|error| optional_read_error(backend, error))?
            .iter()
            .map(|package| {
                package.identity().map(|identity| {
                    identity.with_display_name(format!(
                        "{} {} -> {}",
                        package.name,
                        package.current.as_deref().unwrap_or("?"),
                        package.candidate
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| optional_read_error(backend, error))?,
        _ => {
            let records = match &operation {
                ReadOperation::Search { .. } => parse_search(&output.stdout),
                ReadOperation::Installed => parse_list(&output.stdout),
                ReadOperation::Details { .. } => parse_info(&output.stdout),
                _ => unreachable!(),
            }
            .map_err(|error| optional_read_error(backend, error))?;
            records
                .iter()
                .map(|package| package.identity())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| optional_read_error(backend, error))?
        }
    };
    let details = matches!(operation, ReadOperation::Details { .. }).then_some(ReadDetails {
        stdout: output.stdout,
        stderr: output.stderr,
        status: output.status,
        success: true,
    });
    Ok((packages, CatalogStrategy::DirectQuery, details))
}

fn read_brew(
    operation: ReadOperation,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    use crate::backends::brew::{BrewBackend, BrewKind, identity, parse_info, parse_outdated};
    let backend = BackendId::Brew;
    let provider =
        BrewBackend::from_path(std::env::var_os("PATH").as_deref()).map_err(|error| {
            BackendError::CommandUnavailable {
                backend,
                operation: "read packages",
                command: error.to_string(),
            }
        })?;
    let plan = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog => provider.formulae_plan(),
        ReadOperation::Search { query } => provider
            .search_plan(query)
            .map_err(|error| optional_read_error(backend, error))?,
        ReadOperation::Installed => {
            let mut plan = provider.formulae_plan();
            plan.args = ["info", "--json=v2", "--installed", "--formula"]
                .map(OsString::from)
                .to_vec();
            plan
        }
        ReadOperation::Details { package, scope } => {
            if *scope != PackageScope::Profile {
                return Err(optional_read_error(backend, "brew requires profile scope"));
            }
            provider
                .info_plan(package.as_str(), BrewKind::Formula)
                .map_err(|error| optional_read_error(backend, error))?
        }
        ReadOperation::Updates => provider.outdated_plan(),
    };
    let output = execute_plan(&plan, backend, "read packages")?;
    if !output.status.success() {
        return Err(command_failed(backend, "read packages", &output));
    }
    let packages = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog | ReadOperation::Search { .. } => {
            output
                .stdout
                .split_whitespace()
                .map(|token| {
                    identity_for(
                        backend,
                        PackageKind::BrewFormula,
                        PackageScope::Profile,
                        token.to_owned(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        ReadOperation::Installed | ReadOperation::Details { .. } => {
            parse_info(&output.stdout, BrewKind::Formula)
                .map_err(|error| optional_read_error(backend, error))?
                .iter()
                .map(|package| identity(package, PackageScope::Profile))
                .collect()
        }
        ReadOperation::Updates => parse_outdated(&output.stdout)
            .map_err(|error| optional_read_error(backend, error))?
            .into_iter()
            .map(|package| {
                let display = format!(
                    "{} {} -> {}",
                    package.token,
                    package.current.as_deref().unwrap_or("?"),
                    package.candidate
                );
                identity_for(
                    backend,
                    package.kind.package_kind(),
                    PackageScope::Profile,
                    package.token,
                )
                .map(|identity| identity.with_display_name(display))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let details = matches!(operation, ReadOperation::Details { .. }).then_some(ReadDetails {
        stdout: output.stdout,
        stderr: output.stderr,
        status: output.status,
        success: true,
    });
    Ok((packages, CatalogStrategy::Enumerated, details))
}

fn read_nix(
    operation: ReadOperation,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    use crate::backends::nix::{NixBackend, identity, parse_profile, parse_search};
    let backend = BackendId::Nix;
    let provider = NixBackend::from_path(std::env::var_os("PATH").as_deref()).map_err(|error| {
        BackendError::CommandUnavailable {
            backend,
            operation: "read packages",
            command: error.to_string(),
        }
    })?;
    let plan = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog => {
            return Err(BackendError::QueryRequired { backend });
        }
        ReadOperation::Search { query } => provider
            .search_plan(query)
            .map_err(|error| optional_read_error(backend, error))?,
        ReadOperation::Installed | ReadOperation::Updates => provider.profile_list_plan(),
        ReadOperation::Details { package, scope } => {
            if *scope != PackageScope::Profile {
                return Err(optional_read_error(backend, "nix requires profile scope"));
            }
            provider
                .details_plan(package.as_str())
                .map_err(|error| optional_read_error(backend, error))?
        }
    };
    let output = provider
        .execute(&plan)
        .map_err(|error| optional_read_error(backend, error))?;
    if !output.status.success() {
        return Err(command_failed(backend, "read packages", &output));
    }
    let records = match &operation {
        ReadOperation::Search { .. } => parse_search(&output.stdout),
        _ => parse_profile(&output.stdout),
    }
    .map_err(|error| optional_read_error(backend, error))?;
    let records = if matches!(operation, ReadOperation::Updates) {
        let mut updates = Vec::new();
        for mut record in records.into_iter().filter(|record| record.active) {
            let plan = provider
                .update_candidate_plan(&record)
                .map_err(|error| optional_read_error(backend, error))?;
            let candidate = provider
                .execute(&plan)
                .map_err(|error| optional_read_error(backend, error))?;
            if !candidate.status.success() {
                return Err(command_failed(
                    backend,
                    "check update candidate",
                    &candidate,
                ));
            }
            let store_path: String = serde_json::from_str(&candidate.stdout)
                .map_err(|error| optional_read_error(backend, error))?;
            if !store_path.starts_with("/nix/store/") {
                return Err(optional_read_error(backend, "invalid candidate store path"));
            }
            if !record.store_paths.contains(&store_path) {
                record.name = format!(
                    "{} {} -> {}",
                    record.selector,
                    record.store_paths.join(","),
                    store_path
                );
                updates.push(record);
            }
        }
        updates
    } else {
        records
    };
    let packages = records
        .iter()
        .filter(|record| match &operation {
            ReadOperation::Details { package, .. } => record.selector == package.as_str(),
            _ => true,
        })
        .map(identity)
        .collect();
    let details = matches!(operation, ReadOperation::Details { .. }).then_some(ReadDetails {
        stdout: output.stdout,
        stderr: output.stderr,
        status: output.status,
        success: true,
    });
    Ok((packages, CatalogStrategy::DirectQuery, details))
}

fn read_zypper(
    operation: ReadOperation,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let backend = BackendId::Zypper;
    let provider = ZypperBackend::from_path(std::env::var_os("PATH").as_deref())
        .map_err(|e| optional_read_error(backend, e))?;
    if matches!(operation, ReadOperation::RefreshCatalog) {
        let output = execute_plan(&provider.refresh_plan(), backend, "refresh package catalog")?;
        if !output.status.success() {
            return Err(command_failed(backend, "refresh package catalog", &output));
        }
    }
    let (plan, details) = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog => (provider.list_plan(), false),
        ReadOperation::Search { query } => (provider.search_plan(query), false),
        ReadOperation::Installed => (provider.installed_plan(), false),
        ReadOperation::Updates => (provider.updates_plan(), false),
        ReadOperation::Details { package, .. } => (
            provider
                .details_plan(package)
                .map_err(|e| optional_read_error(backend, e))?,
            true,
        ),
    };
    let output = execute_plan(&plan, backend, "read packages")?;
    if !output.status.success() {
        return Err(command_failed(backend, "read packages", &output));
    }
    let records = match operation {
        ReadOperation::Updates => crate::backends::zypper::parse_updates_xml(&output.stdout)
            .map_err(|e| optional_read_error(backend, e))?
            .into_iter()
            .filter_map(|p| {
                let display = format!(
                    "{} {} -> {}",
                    p.name,
                    p.current.as_deref().unwrap_or("?"),
                    p.candidate
                );
                identity_for(backend, PackageKind::System, PackageScope::System, p.name)
                    .ok()
                    .map(|identity| identity.with_display_name(display))
            })
            .collect(),
        _ => crate::backends::zypper::parse_search_xml(&output.stdout)
            .map_err(|e| optional_read_error(backend, e))?
            .into_iter()
            .filter_map(|p| {
                identity_for(backend, PackageKind::System, PackageScope::System, p.name).ok()
            })
            .collect(),
    };
    let detail = details.then_some(ReadDetails {
        stdout: output.stdout,
        stderr: output.stderr,
        status: output.status,
        success: true,
    });
    Ok((records, CatalogStrategy::Enumerated, detail))
}

fn read_apk(
    operation: ReadOperation,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let backend = BackendId::Apk;
    let provider = ApkBackend::from_path(std::env::var_os("PATH").as_deref())
        .map_err(|e| optional_read_error(backend, e))?;
    if matches!(operation, ReadOperation::RefreshCatalog) {
        let output = execute_plan(
            &provider.update_catalog_plan(),
            backend,
            "refresh package catalog",
        )?;
        if !output.status.success() {
            return Err(command_failed(backend, "refresh package catalog", &output));
        }
    }
    let (plan, details) = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog => (provider.list_plan(), false),
        ReadOperation::Search { query } => (provider.search_plan(query), false),
        ReadOperation::Installed => (provider.installed_plan(), false),
        ReadOperation::Updates => (provider.updates_plan(), false),
        ReadOperation::Details { package, .. } => (
            provider
                .details_plan(package)
                .map_err(|e| optional_read_error(backend, e))?,
            true,
        ),
    };
    let output = execute_plan(&plan, backend, "read packages")?;
    if !output.status.success() {
        return Err(command_failed(backend, "read packages", &output));
    }
    if let ReadOperation::Details { package, scope } = &operation {
        return Ok((
            vec![identity_for(
                backend,
                PackageKind::System,
                *scope,
                package.as_str().to_owned(),
            )?],
            CatalogStrategy::Enumerated,
            Some(ReadDetails {
                stdout: output.stdout,
                stderr: output.stderr,
                status: output.status,
                success: true,
            }),
        ));
    }
    let packages = match operation {
        ReadOperation::Installed => crate::backends::apk::parse_installed(&output.stdout),
        _ => crate::backends::apk::parse_search(&output.stdout),
    }
    .map_err(|e| optional_read_error(backend, e))?
    .into_iter()
    .filter_map(|p| {
        let display = format!("{} {}", p.name, p.version);
        identity_for(backend, PackageKind::System, PackageScope::System, p.name)
            .ok()
            .map(|identity| {
                if matches!(operation, ReadOperation::Updates) {
                    identity.with_display_name(display)
                } else {
                    identity
                }
            })
    })
    .collect();
    let detail = details.then_some(ReadDetails {
        stdout: output.stdout,
        stderr: output.stderr,
        status: output.status,
        success: true,
    });
    Ok((packages, CatalogStrategy::Enumerated, detail))
}

fn read_xbps(
    operation: ReadOperation,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    let backend = BackendId::Xbps;
    let provider = XbpsBackend::from_path(std::env::var_os("PATH").as_deref())
        .map_err(|e| optional_read_error(backend, e))?;
    if matches!(operation, ReadOperation::RefreshCatalog) {
        let output = execute_plan(&provider.sync_plan(), backend, "refresh package catalog")?;
        if !output.status.success() {
            return Err(command_failed(backend, "refresh package catalog", &output));
        }
    }
    let (plan, details) = match &operation {
        ReadOperation::Catalog | ReadOperation::RefreshCatalog => (provider.list_plan(), false),
        ReadOperation::Search { query } => (provider.search_plan(query), false),
        ReadOperation::Installed => (provider.installed_plan(), false),
        ReadOperation::Updates => (provider.updates_plan(), false),
        ReadOperation::Details { package, .. } => (
            provider
                .details_plan(package)
                .map_err(|e| optional_read_error(backend, e))?,
            true,
        ),
    };
    let output = execute_plan(&plan, backend, "read packages")?;
    if !output.status.success() {
        return Err(command_failed(backend, "read packages", &output));
    }
    if let ReadOperation::Details { package, scope } = &operation {
        return Ok((
            vec![identity_for(
                backend,
                PackageKind::System,
                *scope,
                package.as_str().to_owned(),
            )?],
            CatalogStrategy::Enumerated,
            Some(ReadDetails {
                stdout: output.stdout,
                stderr: output.stderr,
                status: output.status,
                success: true,
            }),
        ));
    }
    let packages = if matches!(operation, ReadOperation::Updates) {
        crate::backends::xbps::parse_updates(&output.stdout)
            .map_err(|e| optional_read_error(backend, e))?
            .into_iter()
            .filter_map(|p| {
                let display = format!(
                    "{} {} -> {}",
                    p.name,
                    p.current.as_deref().unwrap_or("?"),
                    p.candidate
                );
                identity_for(backend, PackageKind::System, PackageScope::System, p.name)
                    .ok()
                    .map(|identity| identity.with_display_name(display))
            })
            .collect()
    } else {
        let parsed = match operation {
            ReadOperation::Installed => crate::backends::xbps::parse_installed(&output.stdout),
            _ => crate::backends::xbps::parse_search(&output.stdout),
        }
        .map_err(|e| optional_read_error(backend, e))?;
        parsed
            .into_iter()
            .filter_map(|p| {
                identity_for(backend, PackageKind::System, PackageScope::System, p.name).ok()
            })
            .collect()
    };
    let detail = details.then_some(ReadDetails {
        stdout: output.stdout,
        stderr: output.stderr,
        status: output.status,
        success: true,
    });
    Ok((packages, CatalogStrategy::Enumerated, detail))
}

fn read_flatpak(
    operation: ReadOperation,
    backend: BackendId,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    match operation {
        ReadOperation::Catalog
        | ReadOperation::RefreshCatalog
        | ReadOperation::Installed
        | ReadOperation::Search { .. } => {
            let stdout = if matches!(
                operation,
                ReadOperation::Catalog | ReadOperation::RefreshCatalog
            ) {
                flatpak_catalog_output(backend, matches!(operation, ReadOperation::RefreshCatalog))?
            } else {
                let output = run_backend_command(
                    backend,
                    "list Flatpak applications",
                    &["list", "--app", "--columns=application,origin,name"],
                )?;
                if !output.status.success() {
                    return Err(command_failed(
                        backend,
                        "list Flatpak applications",
                        &output,
                    ));
                }
                output.stdout
            };
            let query = match operation {
                ReadOperation::Search { query } => Some(query),
                _ => None,
            };
            let packages = parse_flatpak_catalog(&stdout, query.as_deref());
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
        ReadOperation::Details { package, scope } => {
            let key = package.as_str().to_owned();
            let args = if scope == PackageScope::System {
                vec!["info".to_owned(), "--system".to_owned(), key.clone()]
            } else {
                vec!["info".to_owned(), key.clone()]
            };
            let output = run_backend_command(
                backend,
                "read Flatpak application details",
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            Ok((
                vec![identity_for(backend, PackageKind::Flatpak, scope, key)?],
                CatalogStrategy::Enumerated,
                Some(ReadDetails {
                    stdout: output.stdout,
                    stderr: output.stderr,
                    status: output.status,
                    success: output.status.success(),
                }),
            ))
        }
        ReadOperation::Updates => {
            let output = run_backend_command(
                backend,
                "list Flatpak updates",
                &["remote-ls", "--updates", "--columns=application,version"],
            )?;
            if !output.status.success() {
                return Err(command_failed(backend, "list Flatpak updates", &output));
            }
            let packages = output
                .stdout
                .lines()
                .filter_map(|line| line.split_whitespace().next().map(|id| (id, line)))
                .filter(|(id, _)| valid_package_token(id))
                .map(|(id, line)| {
                    identity_for(
                        backend,
                        PackageKind::Flatpak,
                        PackageScope::User,
                        id.to_owned(),
                    )
                    .map(|identity| identity.with_display_name(line))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
    }
}

fn parse_flatpak_catalog(text: &str, query: Option<&str>) -> Vec<PackageIdentity> {
    text.lines()
        .filter(|line| {
            query.is_none_or(|query| line.split('\t').any(|field| field.contains(query)))
        })
        .filter_map(parse_flatpak_identity)
        .collect()
}

const FLATPAK_CATALOG_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

fn flatpak_catalog_output(backend: BackendId, refresh: bool) -> Result<String, BackendError> {
    let cache_dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|path| path.join("packtide/flatpak"));
    if !refresh
        && let Some(cache) = cache_dir.as_ref()
        && let Ok(store) = crate::CacheStore::new(cache.clone())
        && let Ok(Some(contents)) = store.read_fresh("catalog", FLATPAK_CATALOG_TTL)
    {
        return Ok(contents);
    }

    let cached = run_backend_command(
        backend,
        "enumerate cached Flatpak applications",
        &[
            "remote-ls",
            "--app",
            "--cached",
            "--columns=application,origin,name",
        ],
    );
    let cached_text = cached
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout);
    let contents = if let Some(contents) = select_flatpak_catalog(cached_text.as_deref(), None) {
        contents
    } else {
        let live = run_backend_command(
            backend,
            "enumerate Flatpak applications",
            &["remote-ls", "--app", "--columns=application,origin,name"],
        )?;
        if !live.status.success() {
            return Err(command_failed(
                backend,
                "enumerate Flatpak applications",
                &live,
            ));
        }
        select_flatpak_catalog(None, Some(&live.stdout)).unwrap_or_default()
    };
    if !contents.trim().is_empty()
        && let Some(cache) = cache_dir
        && let Ok(store) = crate::CacheStore::new(cache)
    {
        let _ = store.write_atomic("catalog", &contents);
    }
    Ok(contents)
}

fn select_flatpak_catalog(cached: Option<&str>, live: Option<&str>) -> Option<String> {
    cached
        .filter(|text| !text.trim().is_empty())
        .or_else(|| live.filter(|text| !text.trim().is_empty()))
        .map(str::to_owned)
}

fn identity_for(
    backend: BackendId,
    kind: PackageKind,
    scope: PackageScope,
    key: String,
) -> Result<PackageIdentity, BackendError> {
    Ok(PackageIdentity::new(
        backend,
        kind,
        scope,
        NativePackageKey::new(key)?,
    ))
}

fn run_backend_command(
    backend: BackendId,
    operation: &'static str,
    args: &[&str],
) -> Result<crate::Output, BackendError> {
    run_backend_command_for(backend, backend.as_str(), operation, args)
}

fn run_backend_command_for(
    backend: BackendId,
    command: &str,
    operation: &'static str,
    args: &[&str],
) -> Result<crate::Output, BackendError> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let Some(path) = resolver.resolve(OsStr::new(command)) else {
        return Err(BackendError::CommandUnavailable {
            backend,
            operation,
            command: command.to_owned(),
        });
    };
    crate::run_capture_path(&path, args, true).map_err(|error| BackendError::CommandFailed {
        backend,
        operation,
        message: error.to_string(),
    })
}

fn command_failed(
    backend: BackendId,
    operation: &'static str,
    output: &crate::Output,
) -> BackendError {
    let message = if output.stderr.trim().is_empty() {
        format!("command exited with {}", output.status)
    } else {
        output.stderr.trim().to_owned()
    };
    BackendError::CommandFailed {
        backend,
        operation,
        message,
    }
}

fn parse_pacman_sync_identity(line: &str) -> Option<PackageIdentity> {
    let mut fields = line.split_whitespace();
    let repo = fields.next()?;
    let name = fields.next()?;
    let _version = fields.next()?;
    if !valid_package_token(repo) || !valid_package_token(name) {
        return None;
    }
    identity_for(
        BackendId::Pacman,
        PackageKind::System,
        PackageScope::System,
        format!("{repo}/{name}"),
    )
    .ok()
}

fn parse_pacman_installed_identity(line: &str) -> Option<PackageIdentity> {
    let name = line.split_whitespace().next()?;
    if !valid_package_token(name) {
        return None;
    }
    identity_for(
        BackendId::Pacman,
        PackageKind::System,
        PackageScope::System,
        name.to_owned(),
    )
    .ok()
}

fn parse_flatpak_identity(line: &str) -> Option<PackageIdentity> {
    let mut fields = line.split('\t');
    let id = fields.next()?.trim();
    let origin = fields.next()?.trim();
    let name = fields.next()?.trim();
    let installation = fields.next().unwrap_or("user").trim();
    if !valid_package_token(id) {
        return None;
    }
    let scope = match installation {
        "system" => PackageScope::System,
        _ => PackageScope::User,
    };
    Some(
        identity_for(
            BackendId::Flatpak,
            PackageKind::Flatpak,
            scope,
            id.to_owned(),
        )
        .ok()?
        .with_origin(origin)
        .with_display_name(name),
    )
}

fn parse_update_identity(line: &str) -> Option<String> {
    let name = line.split_whitespace().next()?.trim();
    valid_package_token(name).then(|| name.to_owned())
}

fn valid_package_token(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._:+/-".contains(&byte))
}

fn aur_names(backend: BackendId) -> Result<String, BackendError> {
    let cache_dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|path| path.join("packtide/aur"));
    if let Some(cache_dir) = cache_dir
        && let Ok(Some(contents)) =
            crate::CacheStore::new(cache_dir).and_then(|cache| cache.read("packages"))
    {
        return Ok(contents);
    }
    let output = run_backend_command_for(
        backend,
        backend.as_str(),
        "enumerate AUR packages",
        &["-Sl"],
    )?;
    if !output.status.success() {
        return Err(command_failed(backend, "enumerate AUR packages", &output));
    }
    let mut names = String::new();
    for line in output.stdout.lines() {
        let mut fields = line.split_whitespace();
        let Some(first) = fields.next() else {
            continue;
        };
        let name = if first == "aur" {
            fields.next()
        } else {
            Some(first)
        };
        if let Some(name) = name.filter(|name| valid_package_token(name)) {
            if !names.is_empty() {
                names.push('\n');
            }
            names.push_str(name);
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::{
        BackendClass, BackendError, BackendId, BackendOperation, BackendResponse, BuiltinBackend,
        CapabilitySet, CatalogStrategy, NativePackageKey, PackageBackend, PackageId,
        PackageIdentity, PackageKind, PackageScope, ReadOperation, Scope, WriteOperation,
        parse_flatpak_catalog,
    };
    use crate::{CommandPrivilege, TransactionAction};

    #[test]
    fn read_plan_applies_environment_and_preserves_failure_output() {
        // Given a read plan removing an inherited variable and forcing a locale.
        assert!(std::env::var_os("HOME").is_some());
        let plan = crate::CommandPlan::new(std::path::PathBuf::from("/bin/sh"))
            .arg("-c")
            .arg("printf '%s\\n' \"${LC_ALL-<unset>}\" \"${HOME-<unset>}\"; printf 'read failure\\n' >&2; exit 7")
            .with_env_remove("HOME")
            .with_locale("C");

        // When the backend executes the plan and captures its output.
        let output = super::execute_plan(&plan, BackendId::Apt, "read packages")
            .expect("capture unsuccessful read command");

        // Then plan policy and both streams survive the execution boundary.
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, "C\n<unset>\n");
        assert_eq!(output.stderr, "read failure\n");
    }

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
            let expected_class = match backend {
                BackendId::Pacman
                | BackendId::Apt
                | BackendId::Dnf5
                | BackendId::Dnf4
                | BackendId::Zypper
                | BackendId::Apk
                | BackendId::Xbps => BackendClass::Native,
                BackendId::Paru
                | BackendId::Yay
                | BackendId::Flatpak
                | BackendId::Snap
                | BackendId::Brew
                | BackendId::Nix => BackendClass::Optional,
            };
            assert_eq!(builtin.class(), expected_class);
            assert!(PackageKind::ALL.contains(&builtin.kind()));
            assert!(PackageScope::ALL.contains(&builtin.scope()));
        }
        assert_eq!(Scope::User, PackageScope::User);
        assert_eq!(
            BuiltinBackend::new(BackendId::Brew).scope(),
            PackageScope::Profile
        );
        assert_eq!(
            BuiltinBackend::new(BackendId::Nix).scope(),
            PackageScope::Profile
        );
        assert!(
            BuiltinBackend::new(BackendId::Apt)
                .capabilities()
                .contains(CapabilitySet::INSTALL)
        );
    }

    #[test]
    fn same_native_name_different_backend_or_scope_is_not_equal() {
        let pacman = PackageIdentity::new(
            BackendId::Pacman,
            PackageKind::System,
            PackageScope::System,
            PackageId::new("same-name").unwrap(),
        );
        let apt = PackageIdentity::new(
            BackendId::Apt,
            PackageKind::System,
            PackageScope::System,
            PackageId::new("same-name").unwrap(),
        );
        let user_scope = PackageIdentity::new(
            BackendId::Pacman,
            PackageKind::System,
            PackageScope::User,
            PackageId::new("same-name").unwrap(),
        );
        assert_ne!(pacman, apt);
        assert_ne!(pacman, user_scope);
        assert_eq!(pacman.key().as_str(), "same-name");
        let native = NativePackageKey::new("same-name").unwrap();
        assert_eq!(native.as_str(), "same-name");
    }

    #[test]
    fn arch_backends_keep_typed_transaction_argv_and_scope() {
        let plan = BuiltinBackend::new(BackendId::Paru)
            .transaction(TransactionAction::Remove, ids(&["aur/tool"]))
            .expect("paru remove plan");
        assert_eq!(plan.scope, PackageScope::User);
        assert_eq!(plan.kind, PackageKind::Aur);
        assert!(plan.command.program.is_absolute());
        assert_eq!(plan.command.args, ["-Rns", "aur/tool"]);
        assert_eq!(
            plan.command.locale.as_deref(),
            Some(std::ffi::OsStr::new("C"))
        );
        assert_eq!(plan.command.privilege, CommandPrivilege::User);
        assert_eq!(plan.packages[0].native_key.as_str(), "aur/tool");
    }

    #[test]
    fn write_plan_uses_absolute_path_from_injected_resolver() {
        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-command-plan-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create resolver fixture");
        let executable = directory.join("paru");
        std::fs::write(&executable, "fixture").expect("write resolver fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&executable)
                .expect("stat resolver fixture")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&executable, permissions).expect("make fixture executable");
        }
        let resolver = crate::ExecutableResolver::from_path(Some(directory.as_os_str()));
        let backend = BuiltinBackend::new(BackendId::Paru);
        let plan = backend
            .command_for_with_resolver(
                &WriteOperation::Install {
                    packages: vec![backend.identity(NativePackageKey::new("aur/tool").unwrap())],
                },
                &resolver,
            )
            .expect("build command plan from injected resolver");
        assert_eq!(plan.program, executable);
        assert!(plan.program.is_absolute());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn flatpak_identity_drives_application_id_argv() {
        const APPLICATION_ID: &str = "org.example.App";
        let backend = BuiltinBackend::new(BackendId::Flatpak);
        let identity = backend.identity(NativePackageKey::new(APPLICATION_ID).unwrap());
        let expected_identity = PackageIdentity::new(
            BackendId::Flatpak,
            PackageKind::Flatpak,
            PackageScope::User,
            NativePackageKey::new(APPLICATION_ID).unwrap(),
        );
        assert_eq!(identity, expected_identity);

        let plan = backend
            .write(WriteOperation::Remove {
                packages: vec![identity],
            })
            .expect("flatpak uninstall plan");
        assert!(plan.command.program.is_absolute());
        assert_eq!(plan.command.args, ["uninstall", "org.example.App"]);
        assert_eq!(plan.packages, vec![expected_identity]);

        let system_identity = PackageIdentity::new(
            BackendId::Flatpak,
            PackageKind::Flatpak,
            PackageScope::System,
            NativePackageKey::new(APPLICATION_ID).unwrap(),
        );
        let system_plan = backend
            .write(WriteOperation::Remove {
                packages: vec![system_identity],
            })
            .expect("system Flatpak uninstall plan");
        assert_eq!(system_plan.scope, PackageScope::System);
        assert_eq!(
            system_plan.command.args,
            ["uninstall", "--system", APPLICATION_ID]
        );
        assert_eq!(system_plan.command.privilege, CommandPrivilege::Elevated);
    }

    #[test]
    fn read_contract_has_distinct_catalog_search_details_installed_and_updates() {
        let backend = BuiltinBackend::new(BackendId::Pacman);
        assert_eq!(
            backend
                .read_packages(ids(&["core/bash"]))
                .unwrap()
                .operation,
            ReadOperation::Catalog
        );
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
            ReadOperation::Details {
                package: PackageId::new("bash").unwrap(),
                scope: PackageScope::System,
            },
            ReadOperation::Details { .. }
        ));
        assert_eq!(ReadOperation::Updates.capability(), CapabilitySet::UPDATES);
    }

    #[test]
    fn refresh_catalog_has_provider_specific_capability_contract() {
        assert_eq!(
            ReadOperation::RefreshCatalog.capability(),
            CapabilitySet::REFRESH_CATALOG
        );
        for backend in [
            BackendId::Apt,
            BackendId::Zypper,
            BackendId::Apk,
            BackendId::Xbps,
        ] {
            assert!(
                BuiltinBackend::new(backend)
                    .capabilities()
                    .contains(CapabilitySet::REFRESH_CATALOG)
            );
        }
        for backend in [BackendId::Dnf4, BackendId::Dnf5] {
            assert_eq!(
                BuiltinBackend::new(backend).read(ReadOperation::RefreshCatalog),
                Err(BackendError::UnsupportedCapability {
                    backend,
                    capability: CapabilitySet::REFRESH_CATALOG,
                })
            );
        }
    }

    #[test]
    fn native_refresh_catalog_runs_provider_refresh_before_listing() {
        if let Ok(backend) = std::env::var("NATIVE_REFRESH_BACKEND") {
            let sudo = std::env::var_os("NATIVE_REFRESH_SUDO").expect("fake trusted sudo path");
            crate::command::TEST_SUDO_PATH.with_borrow_mut(|path| *path = Some(sudo.into()));
            let backend = match backend.as_str() {
                "zypper" => BackendId::Zypper,
                "apk" => BackendId::Apk,
                "xbps" => BackendId::Xbps,
                _ => panic!("unknown backend"),
            };
            let result = BuiltinBackend::new(backend)
                .read(ReadOperation::RefreshCatalog)
                .expect("refresh and list commands succeed");
            assert_eq!(result.packages.len(), 1);
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("system-tools-core-refresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let zypper = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$NATIVE_REFRESH_LOG\"\nif [ \"$1\" = refresh ]; then exit 0; fi\nprintf '%s' '{}'\n",
            include_str!("../../../tests/package-managers/fixtures/zypper/search.xml")
        );
        let apk = "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$NATIVE_REFRESH_LOG\"\nif [ \"$1\" = update ]; then exit 0; fi\nprintf 'libfoo-bar\\t2:1.4.0-r3\\tx86_64\\talpine-main\\n'\n";
        let xbps_query = "#!/bin/sh\nprintf 'query %s\\n' \"$*\" >> \"$NATIVE_REFRESH_LOG\"\nprintf 'lib-foo-1.2_2\\trepo-main\\tx86_64\\n'\n";
        let xbps_install = "#!/bin/sh\nprintf 'install %s\\n' \"$*\" >> \"$NATIVE_REFRESH_LOG\"\n[ \"$1\" = -S ]\n";
        let xbps_remove = "#!/bin/sh\nexit 0\n";
        for (name, body) in [
            (
                "sudo",
                "#!/bin/sh\nprintf 'sudo %s\\n' \"$*\" >> \"$NATIVE_REFRESH_LOG\"\nexec \"$@\"\n",
            ),
            ("zypper", zypper.as_str()),
            ("apk", apk),
            ("xbps-query", xbps_query),
            ("xbps-install", xbps_install),
            ("xbps-remove", xbps_remove),
        ] {
            let path = directory.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        for (backend, expected) in [
            ("zypper", vec!["refresh", "--xmlout search -s -t package"]),
            ("apk", vec!["update", "search --no-cache *"]),
            ("xbps", vec!["install -S", "query -Rs ."]),
        ] {
            let log = directory.join(format!("{backend}.log"));
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "backend::tests::native_refresh_catalog_runs_provider_refresh_before_listing",
                    "--nocapture",
                ])
                .env("NATIVE_REFRESH_BACKEND", backend)
                .env("NATIVE_REFRESH_LOG", &log)
                .env("NATIVE_REFRESH_SUDO", directory.join("sudo"))
                .env("PATH", &directory)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{backend}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let actual = std::fs::read_to_string(log).unwrap();
            let (program, argument) = match backend {
                "zypper" => ("zypper", "refresh"),
                "apk" => ("apk", "update"),
                "xbps" => ("xbps-install", "-S"),
                _ => unreachable!("fixture backend"),
            };
            let sudo_invocation = format!("sudo {} {argument}", directory.join(program).display());
            let expected = std::iter::once(sudo_invocation.as_str())
                .chain(expected)
                .collect::<Vec<_>>();
            assert_eq!(actual.lines().collect::<Vec<_>>(), expected);
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn flatpak_catalog_is_enumerated_and_unsupported_write_fails_before_execution() {
        let flatpak = BuiltinBackend::new(BackendId::Flatpak);
        let catalog = flatpak
            .read_packages(ids(&["org.example.App"]))
            .expect("Flatpak identity catalog");
        assert_eq!(catalog.source, CatalogStrategy::Enumerated);
        assert_eq!(catalog.packages[0].native_key.as_str(), "org.example.App");
        assert_eq!(
            flatpak.write(WriteOperation::Downgrade {
                packages: vec![flatpak.identity(NativePackageKey::new("org.example.App").unwrap())]
            }),
            Err(BackendError::UnsupportedCapability {
                backend: BackendId::Flatpak,
                capability: CapabilitySet::DOWNGRADE
            })
        );
        assert_eq!(
            flatpak.write(WriteOperation::SystemUpgrade),
            Err(BackendError::UnsupportedCapability {
                backend: BackendId::Flatpak,
                capability: CapabilitySet::SYSTEM_UPGRADE
            })
        );
    }

    #[test]
    fn flatpak_empty_installed_catalog_has_no_first_line_identity() {
        assert!(parse_flatpak_catalog("", None).is_empty());
        assert!(parse_flatpak_catalog("\n", None).is_empty());
    }

    #[test]
    fn flatpak_catalog_filters_query_without_reinterpreting_display_name() {
        let packages = parse_flatpak_catalog(
            "org.example.App\tflathub\tDemo App\tuser\norg.other.Tool\tflathub\tTool\tuser",
            Some("Demo"),
        );
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].native_key.as_str(), "org.example.App");
    }

    #[test]
    fn flatpak_catalog_selection_prefers_cache_then_live_and_rejects_empty() {
        assert_eq!(
            super::select_flatpak_catalog(Some("cached"), Some("live")),
            Some("cached".to_owned())
        );
        assert_eq!(
            super::select_flatpak_catalog(Some("\n"), Some("live")),
            Some("live".to_owned())
        );
        assert_eq!(super::select_flatpak_catalog(None, Some("\n")), None);
    }

    #[test]
    fn flatpak_installed_rows_preserve_system_scope() {
        let rows = super::parse_flatpak_catalog(
            "org.example.User\tflathub\tUser Display\tuser\norg.example.System\tflathub\tSystem Display\tsystem",
            None,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].scope, PackageScope::User);
        assert_eq!(rows[1].scope, PackageScope::System);
    }

    #[test]
    fn unsupported_write_is_rejected_before_command_spawn() {
        let backend = BuiltinBackend::new(BackendId::Apt);
        let operation = WriteOperation::Downgrade {
            packages: vec![backend.identity(NativePackageKey::new("same-name").unwrap())],
        };

        assert_eq!(
            backend.write(operation),
            Err(BackendError::UnsupportedCapability {
                backend: BackendId::Apt,
                capability: CapabilitySet::DOWNGRADE,
            })
        );
    }

    #[test]
    fn dispatch_keeps_read_and_write_response_types_separate() {
        let backend = BuiltinBackend::new(BackendId::Pacman);
        let read = BackendResponse::Read(
            backend
                .read_packages(ids(&["core/bash"]))
                .expect("typed read response"),
        );
        assert!(matches!(read, BackendResponse::Read(_)));
        let write = backend
            .dispatch(BackendOperation::Write(WriteOperation::Install {
                packages: vec![backend.identity(NativePackageKey::new("core/bash").unwrap())],
            }))
            .unwrap();
        assert!(matches!(write, BackendResponse::Write(_)));
    }

    #[test]
    fn write_rejects_identity_from_another_backend_before_building_argv() {
        let pacman = BuiltinBackend::new(BackendId::Pacman);
        let paru = BuiltinBackend::new(BackendId::Paru);
        let error = pacman
            .write(WriteOperation::Install {
                packages: vec![paru.identity(NativePackageKey::new("same-name").unwrap())],
            })
            .unwrap_err();
        assert_eq!(
            error,
            BackendError::IdentityMismatch {
                expected_backend: BackendId::Pacman,
                actual_backend: BackendId::Paru,
                expected_kind: PackageKind::System,
                actual_kind: PackageKind::Aur,
                expected_scope: PackageScope::System,
                actual_scope: PackageScope::User,
            }
        );
    }

    #[test]
    fn command_for_rejects_identity_tampering_before_provider_argv() {
        for backend in [BackendId::Zypper, BackendId::Apk, BackendId::Xbps] {
            let foreign_identity = BuiltinBackend::new(BackendId::Pacman)
                .identity(NativePackageKey::new("same-name").unwrap());
            let provider = BuiltinBackend::new(backend);
            let operation = WriteOperation::Install {
                packages: vec![foreign_identity.clone()],
            };
            assert!(matches!(
                provider.command_for(&operation),
                Err(BackendError::IdentityMismatch { .. })
            ));
        }
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
    fn native_provider_dispatch_uses_resolved_provider_argv() {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("native-dispatch-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for executable in ["apt-get", "dnf", "dnf5"] {
            let path = directory.join(executable);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let resolver = crate::ExecutableResolver::from_path(Some(directory.as_os_str()));
        for (id, executable) in [
            (BackendId::Apt, "apt-get"),
            (BackendId::Dnf4, "dnf"),
            (BackendId::Dnf5, "dnf5"),
        ] {
            let backend = BuiltinBackend::new(id);
            let operation = WriteOperation::Install {
                packages: vec![backend.identity(NativePackageKey::new("hello").unwrap())],
            };
            let transaction = backend.write_with_resolver(operation, &resolver).unwrap();
            assert_eq!(transaction.command.program, directory.join(executable));
            assert_eq!(transaction.command.args, ["install", "hello"]);
            assert_eq!(transaction.command.privilege, CommandPrivilege::Elevated);
            println!(
                "{id:?} resolved={} argv={:?}",
                transaction.command.program.display(),
                transaction.command.args
            );
        }
    }

    #[test]
    fn xbps_write_dispatch_uses_real_provider_executables() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-xbps-dispatch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create XBPS resolver fixture");
        for executable in ["xbps-query", "xbps-install", "xbps-remove"] {
            let path = directory.join(executable);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write XBPS fixture");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make XBPS fixture executable");
        }
        let resolver = crate::ExecutableResolver::from_path(Some(directory.as_os_str()));
        let backend = BuiltinBackend::new(BackendId::Xbps);
        let identity = backend.identity(NativePackageKey::new("hello").unwrap());

        let install = backend
            .command_for_with_resolver(
                &WriteOperation::Install {
                    packages: vec![identity.clone()],
                },
                &resolver,
            )
            .expect("resolve xbps-install without a synthetic xbps command");
        assert_eq!(install.program, directory.join("xbps-install"));
        assert_eq!(install.args, ["hello"]);

        let remove = backend
            .command_for_with_resolver(
                &WriteOperation::Remove {
                    packages: vec![identity],
                },
                &resolver,
            )
            .expect("resolve xbps-remove without a synthetic xbps command");
        assert_eq!(remove.program, directory.join("xbps-remove"));
        assert_eq!(remove.args, ["-y", "hello"]);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn dnf_read_dispatch_uses_requested_generation_with_both_commands_present() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-dnf-read-dispatch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create DNF resolver fixture");
        let fixtures = [
            (
                "dnf",
                "#!/bin/sh\nprintf 'legacy-bash\\t0\\t5.2\\t1\\tx86_64\\tfedora\\t1\\n'\n",
            ),
            (
                "dnf5",
                "#!/bin/sh\nprintf 'modern-bash\\t0\\t5.2\\t1\\tx86_64\\tfedora\\t1\\n'\n",
            ),
        ];
        for (executable, body) in fixtures {
            let path = directory.join(executable);
            std::fs::write(&path, body).expect("write DNF fixture");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make DNF fixture executable");
        }
        let resolver = crate::ExecutableResolver::from_path(Some(directory.as_os_str()));

        let dnf4 =
            super::read_dnf_with_resolver(ReadOperation::Installed, BackendId::Dnf4, &resolver)
                .expect("DNF4 read selects dnf when dnf5 is also installed");
        assert_eq!(
            dnf4.0
                .first()
                .expect("DNF4 fixture package")
                .native_key
                .as_str(),
            "legacy-bash"
        );

        let dnf5 =
            super::read_dnf_with_resolver(ReadOperation::Installed, BackendId::Dnf5, &resolver)
                .expect("DNF5 read selects dnf5 when both generations are installed");
        assert_eq!(
            dnf5.0
                .first()
                .expect("DNF5 fixture package")
                .native_key
                .as_str(),
            "modern-bash"
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn dnf_read_dispatch_parses_exit_100_updates_and_preserves_other_failures() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "system-tools-core-dnf-read-status-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create DNF status fixture");
        let fixture = r#"#!/bin/sh
case "$1:$2" in
    repoquery:--upgrades) printf 'bash\t0\t5.3\t1\tx86_64\tfedora\t0\n'; exit 100 ;;
esac
printf '%s\n' 'dnf read failed' >&2
exit 2
"#;
        for executable in ["dnf", "dnf5"] {
            let path = directory.join(executable);
            std::fs::write(&path, fixture).expect("write DNF status fixture");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make DNF status fixture executable");
        }
        let resolver = crate::ExecutableResolver::from_path(Some(directory.as_os_str()));

        for backend in [BackendId::Dnf4, BackendId::Dnf5] {
            let result = super::read_dnf_with_resolver(ReadOperation::Updates, backend, &resolver)
                .expect("DNF exit 100 means updates are available");
            assert_eq!(result.0[0].native_key.as_str(), "bash");
            assert_eq!(result.0[0].display_name.as_deref(), Some("bash 5.3"));
            assert_eq!(result.1, CatalogStrategy::Enumerated);

            let error = super::read_dnf_with_resolver(ReadOperation::Installed, backend, &resolver)
                .expect_err("DNF exit 2 must remain an error");
            assert_eq!(
                error,
                BackendError::CommandFailed {
                    backend,
                    operation: "read packages",
                    message: "dnf read failed".into(),
                }
            );
        }

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn optional_backend_dispatch_uses_provider_plans_and_parsers() {
        if std::env::var_os("OPTIONAL_BACKEND_DISPATCH_CHILD").is_some() {
            for (id, key, expected) in [
                (
                    BackendId::Snap,
                    "hello@latest/stable#strict",
                    vec!["install", "hello"],
                ),
                (BackendId::Brew, "hello", vec!["install", "hello"]),
                (
                    BackendId::Nix,
                    "hello",
                    vec![
                        "--extra-experimental-features",
                        "nix-command flakes",
                        "profile",
                        "install",
                        "hello",
                    ],
                ),
            ] {
                let backend = BuiltinBackend::new(id);
                let records = backend.installed().unwrap();
                assert_eq!(records.packages.len(), 1);
                assert_eq!(records.packages[0].backend, id);
                assert_eq!(records.packages[0].scope, backend.scope());
                let operation = WriteOperation::Install {
                    packages: vec![backend.identity(NativePackageKey::new(key).unwrap())],
                };
                let transaction = backend.write(operation).unwrap();
                assert_eq!(transaction.command.args, expected);
                assert_eq!(
                    transaction.command.privilege,
                    if id == BackendId::Snap {
                        CommandPrivilege::Elevated
                    } else {
                        CommandPrivilege::User
                    }
                );
                println!(
                    "{id:?}: installed={} args={:?}",
                    records.packages.len(),
                    transaction.command.args
                );
            }
            let nix = BuiltinBackend::new(BackendId::Nix);
            let result = nix.search("hello").unwrap();
            assert_eq!(
                result.packages[0].native_key.as_str(),
                "nixpkgs#legacyPackages.x86_64-linux.hello"
            );
            assert!(nix.capabilities().contains(CapabilitySet::UPDATES));
            assert!(matches!(
                nix.catalog(),
                Err(BackendError::QueryRequired { .. })
            ));
            let snap = BuiltinBackend::new(BackendId::Snap);
            assert!(matches!(
                snap.catalog(),
                Err(BackendError::QueryRequired { .. })
            ));
            let classic =
                snap.identity(NativePackageKey::new("hello@latest/stable#classic").unwrap());
            assert!(matches!(
                snap.write(WriteOperation::Install {
                    packages: vec![classic]
                }),
                Err(BackendError::CommandFailed { .. })
            ));
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("optional-backend-dispatch-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for (name, script) in [
            (
                "snap",
                "#!/bin/sh\nprintf 'Name Version Rev Tracking Publisher Notes\\nhello 1.0 1 latest/stable publisher -\\n'\n",
            ),
            (
                "brew",
                "#!/bin/sh\nprintf '%s\\n' '{\"formulae\":[{\"name\":\"hello\",\"full_name\":\"homebrew/core/hello\",\"versions\":{\"stable\":\"1.0\"}}]}'\n",
            ),
            (
                "nix",
                "#!/bin/sh\ncase \"$3\" in\nsearch) printf '%s\\n' '{\"legacyPackages.x86_64-linux.hello\":{\"pname\":\"hello\",\"version\":\"1.0\"}}';;\n*) printf '%s\\n' '[{\"name\":\"hello\",\"attrPath\":\"hello\",\"active\":true}]';;\nesac\n",
            ),
        ] {
            let path = directory.join(name);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backend::tests::optional_backend_dispatch_uses_provider_plans_and_parsers",
                "--nocapture",
            ])
            .env("OPTIONAL_BACKEND_DISPATCH_CHILD", "1")
            .env("PATH", &directory)
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn catalog_strategy_declares_provider_query_requirements() {
        assert_eq!(
            BuiltinBackend::new(BackendId::Pacman).catalog_strategy(),
            CatalogStrategy::Enumerated
        );
        assert_eq!(
            BuiltinBackend::new(BackendId::Flatpak).catalog_strategy(),
            CatalogStrategy::Enumerated
        );
        assert_eq!(
            BuiltinBackend::new(BackendId::Snap).catalog_strategy(),
            CatalogStrategy::DirectQuery
        );
        assert_eq!(
            BuiltinBackend::new(BackendId::Nix).catalog_strategy(),
            CatalogStrategy::DirectQuery
        );
    }

    #[test]
    fn typed_read_parsers_preserve_source_identity_and_reject_malformed_rows() {
        let pacman = super::parse_pacman_sync_identity("core bash 5.3-1").expect("pacman sync row");
        assert_eq!(pacman.native_key.as_str(), "core/bash");
        assert_eq!(pacman.scope, PackageScope::System);
        assert_eq!(super::parse_pacman_sync_identity("broken"), None);

        let flatpak = super::parse_flatpak_identity("org.example.App\tflathub\tDemo").unwrap();
        assert_eq!(flatpak.native_key.as_str(), "org.example.App");
        assert_eq!(flatpak.scope, PackageScope::User);
        assert_eq!(flatpak.origin.as_deref(), Some("flathub"));
        assert_eq!(flatpak.display_name.as_deref(), Some("Demo"));
        let system =
            super::parse_flatpak_identity("org.example.System\tflathub\tSystem Demo\tsystem")
                .unwrap();
        assert_eq!(system.scope, PackageScope::System);
        assert_eq!(super::parse_flatpak_identity("\tflathub\tmissing"), None);
    }

    #[test]
    fn typed_read_identity_does_not_use_display_labels() {
        let identity =
            super::parse_flatpak_identity("org.example.App\tflathub\tA misleading display label")
                .unwrap();
        assert_eq!(identity.native_key.as_str(), "org.example.App");
        let plan = BuiltinBackend::new(BackendId::Flatpak)
            .write(WriteOperation::Remove {
                packages: vec![identity],
            })
            .unwrap();
        assert_eq!(plan.command.args, ["uninstall", "org.example.App"]);
    }
}
