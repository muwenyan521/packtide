use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;

use crate::ExecutableResolver;
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
        if matches!(operation, ReadOperation::Catalog)
            && self.catalog_strategy().is_query_required()
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
        match self.0 {
            BackendId::Flatpak => PackageKind::Flatpak,
            BackendId::Snap => PackageKind::Snap,
            BackendId::Brew => PackageKind::BrewFormula,
            BackendId::Nix => PackageKind::Nix,
            BackendId::Paru | BackendId::Yay => PackageKind::Aur,
            BackendId::Pacman
            | BackendId::Apt
            | BackendId::Dnf5
            | BackendId::Dnf4
            | BackendId::Zypper
            | BackendId::Apk
            | BackendId::Xbps => PackageKind::System,
        }
    }

    fn scope(&self) -> PackageScope {
        if matches!(
            self.0,
            BackendId::Pacman
                | BackendId::Apt
                | BackendId::Dnf5
                | BackendId::Dnf4
                | BackendId::Zypper
                | BackendId::Apk
                | BackendId::Xbps
        ) {
            PackageScope::System
        } else if matches!(self.0, BackendId::Brew | BackendId::Nix) {
            PackageScope::Profile
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
            BackendId::Pacman | BackendId::Paru | BackendId::Yay => write
                .union(CapabilitySet::DOWNGRADE)
                .union(CapabilitySet::SYSTEM_UPGRADE),
            _ => CapabilitySet::empty(),
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
        if matches!(operation, ReadOperation::Catalog)
            && self.catalog_strategy().is_query_required()
        {
            return Err(BackendError::QueryRequired { backend: self.id() });
        }

        let (packages, source, details) = match self.0 {
            BackendId::Pacman => read_pacman(operation.clone(), self.id())?,
            BackendId::Paru | BackendId::Yay => read_aur(operation.clone(), self.id())?,
            BackendId::Flatpak => read_flatpak(operation.clone(), self.id())?,
            _ => {
                return Ok(ReadResult {
                    backend: self.id(),
                    operation,
                    packages: Vec::new(),
                    source: self.catalog_strategy(),
                    details: None,
                });
            }
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
        ReadOperation::Catalog | ReadOperation::Search { .. } => {
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
                .filter_map(parse_update_identity)
                .map(|name| identity_for(backend, PackageKind::System, PackageScope::System, name))
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
        ReadOperation::Catalog | ReadOperation::Search { .. } => {
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
                .filter_map(parse_update_identity)
                .map(|name| identity_for(backend, PackageKind::Aur, PackageScope::User, name))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
    }
}

fn read_flatpak(
    operation: ReadOperation,
    backend: BackendId,
) -> Result<(Vec<PackageIdentity>, CatalogStrategy, Option<ReadDetails>), BackendError> {
    match operation {
        ReadOperation::Catalog | ReadOperation::Installed | ReadOperation::Search { .. } => {
            let output = run_backend_command(
                backend,
                "list Flatpak applications",
                &[
                    "list",
                    "--app",
                    "--columns=application,origin,name,installation",
                ],
            )?;
            if !output.status.success() {
                return Err(command_failed(
                    backend,
                    "list Flatpak applications",
                    &output,
                ));
            }
            let query = match operation {
                ReadOperation::Search { query } => Some(query),
                _ => None,
            };
            let packages = output
                .stdout
                .lines()
                .filter(|line| {
                    query
                        .as_deref()
                        .is_none_or(|query| line.split('\t').any(|field| field.contains(query)))
                })
                .filter_map(parse_flatpak_identity)
                .collect();
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
                .filter_map(|line| line.split_whitespace().next())
                .filter(|id| valid_package_token(id))
                .map(|id| {
                    identity_for(
                        backend,
                        PackageKind::Flatpak,
                        PackageScope::User,
                        id.to_owned(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((packages, CatalogStrategy::Enumerated, None))
        }
    }
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
        assert_eq!(
            BuiltinBackend::new(BackendId::Apt).capabilities().bits(),
            CapabilitySet::empty().bits()
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
    fn unsupported_write_is_rejected_before_command_spawn() {
        let backend = BuiltinBackend::new(BackendId::Apt);
        let operation = WriteOperation::Install {
            packages: vec![backend.identity(NativePackageKey::new("same-name").unwrap())],
        };

        assert_eq!(
            backend.write(operation),
            Err(BackendError::UnsupportedCapability {
                backend: BackendId::Apt,
                capability: CapabilitySet::INSTALL,
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
    fn command_for_rejects_unimplemented_backends_before_identity_or_argv() {
        let unsupported = [
            BackendId::Apt,
            BackendId::Dnf5,
            BackendId::Dnf4,
            BackendId::Zypper,
            BackendId::Apk,
            BackendId::Xbps,
            BackendId::Snap,
            BackendId::Brew,
            BackendId::Nix,
        ];
        let foreign_identity = BuiltinBackend::new(BackendId::Pacman)
            .identity(NativePackageKey::new("same-name").unwrap());

        for backend in unsupported {
            let provider = BuiltinBackend::new(backend);
            let operation = WriteOperation::Install {
                packages: vec![foreign_identity.clone()],
            };
            assert_eq!(
                provider.command_for(&operation),
                Err(BackendError::UnsupportedCapability {
                    backend,
                    capability: CapabilitySet::INSTALL,
                })
            );
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
