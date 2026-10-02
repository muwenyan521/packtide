use std::ffi::OsString;
use std::path::PathBuf;

use crate::{BackendId, CommandPrivilege};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    pub backend: Option<BackendId>,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env_remove: Vec<OsString>,
    pub locale: Option<OsString>,
    pub privilege: CommandPrivilege,
}

impl CommandPlan {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            backend: None,
            args: Vec::new(),
            env_remove: Vec::new(),
            locale: None,
            privilege: CommandPrivilege::User,
        }
    }

    pub fn with_backend(mut self, backend: BackendId) -> Self {
        self.backend = Some(backend);
        self
    }

    pub fn with_env_remove(mut self, variable: impl Into<OsString>) -> Self {
        self.env_remove.push(variable.into());
        self
    }

    pub fn with_locale(mut self, locale: impl Into<OsString>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    pub fn with_privilege(mut self, privilege: CommandPrivilege) -> Self {
        self.privilege = privilege;
        self
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheDecision {
    Fresh,
    StaleWithValue,
    Missing,
    RefreshInProgress,
    RefreshFailedWithOldValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceTiming {
    pub source: &'static str,
    pub elapsed_ms: u128,
}

#[cfg(test)]
mod tests {
    use super::CommandPlan;
    use crate::CommandPrivilege;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn command_plan_keeps_structured_argv() {
        let plan = CommandPlan::new(PathBuf::from("/usr/bin/tool")).arg("--color=never");
        assert_eq!(plan.program, PathBuf::from("/usr/bin/tool"));
        assert_eq!(plan.args, ["--color=never"]);
        assert_eq!(plan.env_remove, Vec::<OsString>::new());
        assert_eq!(plan.locale, None);
        assert_eq!(plan.privilege, CommandPrivilege::User);
    }

    #[test]
    fn command_plan_declares_environment_locale_and_privilege() {
        let plan = CommandPlan::new(PathBuf::from("/usr/bin/tool"))
            .with_env_remove("LD_PRELOAD")
            .with_locale("C")
            .with_privilege(CommandPrivilege::Elevated)
            .arg(OsString::from("--machine-readable"));

        assert_eq!(plan.env_remove, [OsString::from("LD_PRELOAD")]);
        assert_eq!(plan.locale, Some(OsString::from("C")));
        assert_eq!(plan.privilege, CommandPrivilege::Elevated);
        assert_eq!(plan.args, [OsString::from("--machine-readable")]);
    }
}
