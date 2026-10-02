mod backend;
mod cache;
mod command;
mod executable;
mod package;
mod plan;
mod platform;
mod transaction;

pub use backend::{
    BackendClass, BackendError, BackendId, BackendOperation, BackendResponse, BuiltinBackend,
    CapabilitySet, CatalogStrategy, NativeKey, NativePackageKey, PackageBackend, PackageId,
    PackageIdentity, PackageKind, PackageScope, ReadBackend, ReadDetails, ReadOperation,
    ReadRequest, ReadResult, Scope, TransactionPlan, WriteBackend, WriteOperation,
};
pub use cache::{CacheStore, RefreshLock};
pub use command::{
    Output, PrivilegeRunner, require_command, require_command_for, require_privileged,
    resolve_command_for, run_capture, run_capture_no_args, run_capture_path, run_command_plan,
    run_privileged, run_status, run_status_no_args, run_status_path,
};
pub use executable::{ExecutableResolver, command_exists, current_executable};
pub use package::{PackageSource, TransactionAction};
pub use plan::{CacheDecision, CommandPlan, SourceTiming};
pub use platform::{
    NativeBackend, PlatformError, backend_from_os_release, detect_native_backend,
    detect_native_backend_from_file, detect_native_backend_from_path, parse_os_release,
};
pub use transaction::{
    CommandPrivilege, PackageUpgradeCommand, PackageUpgradePrivilege, package_upgrade_command,
    package_upgrade_command_for, run_package_upgrade, run_package_upgrade_with_resolver,
};
