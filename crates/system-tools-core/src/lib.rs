mod backend;
mod cache;
mod command;
mod executable;
mod package;
mod plan;
mod transaction;

pub use backend::{
    BackendClass, BackendError, BackendId, BuiltinBackend, CapabilitySet, CatalogStrategy,
    PackageBackend, PackageId, PackageKind, ReadResult, Scope, TransactionPlan,
};
pub use cache::{CacheStore, RefreshLock};
pub use command::{
    Output, PrivilegeRunner, require_command, require_command_for, require_privileged,
    resolve_command_for, run_capture, run_capture_no_args, run_capture_path, run_privileged,
    run_status, run_status_no_args, run_status_path,
};
pub use executable::{ExecutableResolver, command_exists, current_executable};
pub use package::{PackageSource, TransactionAction};
pub use plan::{CacheDecision, CommandPlan, SourceTiming};
pub use transaction::{
    PackageUpgradeCommand, PackageUpgradePrivilege, package_upgrade_command,
    package_upgrade_command_for, run_package_upgrade,
};
