use std::collections::HashMap;

use crate::{
    BackendError, BackendId, BackendOperation, BackendResponse, BuiltinBackend, ExecutableResolver,
    PackageBackend, PackageIdentity, TransactionPlan, WriteOperation,
};

pub struct BackendRegistry {
    backends: HashMap<BackendId, BuiltinBackend>,
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl BackendRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self {
            backends: HashMap::new(),
        };
        for id in BackendId::ALL {
            registry.register(id);
        }
        registry
    }

    pub fn register(&mut self, id: BackendId) {
        self.backends.insert(id, BuiltinBackend::new(id));
    }

    pub fn backend(&self, id: BackendId) -> Option<&BuiltinBackend> {
        self.backends.get(&id)
    }

    pub fn dispatch(
        &self,
        backend: BackendId,
        operation: BackendOperation,
    ) -> Result<BackendResponse, BackendError> {
        self.backend(backend)
            .ok_or(BackendError::CommandUnavailable {
                backend,
                operation: "dispatch operation",
                command: backend.as_str().to_owned(),
            })?
            .dispatch(operation)
    }

    pub fn write_with_resolver(
        &self,
        operation: WriteOperation,
        resolver: &ExecutableResolver,
    ) -> Result<TransactionPlan, BackendError> {
        let backend = operation
            .packages()
            .first()
            .map(|package| package.backend)
            .ok_or(BackendError::InvalidPlan)?;
        self.backend(backend)
            .ok_or(BackendError::CommandUnavailable {
                backend,
                operation: "write transaction",
                command: backend.as_str().to_owned(),
            })?
            .write_with_resolver(operation, resolver)
    }

    pub fn route_identity(
        &self,
        identity: &PackageIdentity,
    ) -> Result<&BuiltinBackend, BackendError> {
        self.backend(identity.backend)
            .ok_or(BackendError::CommandUnavailable {
                backend: identity.backend,
                operation: "route package identity",
                command: identity.backend.as_str().to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageId, PackageKind, PackageScope, WriteOperation};

    #[test]
    fn routes_multi_backend_identities_by_typed_id() {
        let mut registry = BackendRegistry::default();
        registry.register(BackendId::Apt);
        registry.register(BackendId::Brew);
        for backend in [BackendId::Apt, BackendId::Brew] {
            let identity = PackageIdentity::new(
                backend,
                PackageKind::System,
                PackageScope::System,
                PackageId::new("native-key").unwrap(),
            );
            assert_eq!(registry.route_identity(&identity).unwrap().id(), backend);
        }
    }

    #[test]
    fn write_rejects_identity_tampering_before_command_resolution() {
        let registry = BackendRegistry::default();
        let package = PackageIdentity::new(
            BackendId::Apt,
            PackageKind::Aur,
            PackageScope::System,
            PackageId::new("display-label-not-a-key").unwrap(),
        );
        let operation = WriteOperation::Install {
            packages: vec![package],
        };
        let resolver = ExecutableResolver::from_path(None);
        let error = registry
            .write_with_resolver(operation, &resolver)
            .expect_err("tampered identity must be rejected before resolving apt-get");
        assert!(matches!(
            error,
            BackendError::IdentityMismatch {
                expected_backend: BackendId::Apt,
                expected_kind: PackageKind::System,
                actual_kind: PackageKind::Aur,
                ..
            }
        ));
    }
}
