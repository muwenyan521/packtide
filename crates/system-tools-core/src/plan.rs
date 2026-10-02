use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    pub backend: Option<crate::BackendId>,
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

impl CommandPlan {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            backend: None,
            args: Vec::new(),
        }
    }

    pub fn with_backend(mut self, backend: crate::BackendId) -> Self {
        self.backend = Some(backend);
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
    use super::{CacheDecision, CommandPlan};
    use std::path::PathBuf;

    #[test]
    fn command_plan_keeps_structured_argv() {
        let plan = CommandPlan::new(PathBuf::from("/usr/bin/tool")).arg("--color=never");
        assert_eq!(plan.program, PathBuf::from("/usr/bin/tool"));
        assert_eq!(plan.args, ["--color=never"]);
    }

    #[test]
    fn cache_decision_variants_are_explicit() {
        assert_ne!(CacheDecision::Fresh, CacheDecision::Missing);
    }
}
