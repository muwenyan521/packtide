use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, PackageId, PackageIdentity,
    PackageKind, PackageScope, TransactionPlan, WriteOperation,
};
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NixPackage {
    pub selector: String,
    pub attr: String,
    pub original_url: Option<String>,
    pub name: String,
    pub store_paths: Vec<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NixError {
    MalformedJson(String),
    MissingField { record: usize, field: &'static str },
    InvalidSelector,
    MissingExecutable(&'static str),
    ExecutionFailed(String),
    ExperimentalFeaturesDisabled,
    UnsupportedOperation,
}
impl fmt::Display for NixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedJson(e) => write!(f, "invalid nix JSON: {e}"),
            Self::MissingField { record, field } => {
                write!(f, "nix record {record} is missing {field}")
            }
            Self::InvalidSelector => f.write_str("nix profile selector must not be empty"),
            Self::MissingExecutable(n) => write!(f, "required executable not found: {n}"),
            Self::ExecutionFailed(e) => write!(f, "failed to execute nix: {e}"),
            Self::ExperimentalFeaturesDisabled => f.write_str("nix-command and flakes experimental features are required; nix.conf was not changed"),
            Self::UnsupportedOperation => f.write_str("nix operation is unsupported"),
        }
    }
}
impl std::error::Error for NixError {}
fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|x| !x.trim().is_empty())
        .map(str::to_owned)
}
pub fn parse_profile(input: &str) -> Result<Vec<NixPackage>, NixError> {
    let root: Value =
        serde_json::from_str(input).map_err(|e| NixError::MalformedJson(e.to_string()))?;
    let xs = root
        .as_array()
        .or_else(|| root.get("elements").and_then(Value::as_array))
        .ok_or_else(|| NixError::MalformedJson("expected profile elements array".into()))?;
    xs.iter()
        .enumerate()
        .map(|(i, v)| {
            let selector = s(v, "name")
                .or_else(|| s(v, "element"))
                .or_else(|| s(v, "index"))
                .ok_or(NixError::MissingField {
                    record: i + 1,
                    field: "name/element",
                })?;
            let attr = s(v, "attrPath")
                .or_else(|| s(v, "attr"))
                .ok_or(NixError::MissingField {
                    record: i + 1,
                    field: "attrPath",
                })?;
            let paths = v
                .get("storePaths")
                .or_else(|| v.get("store_paths"))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            Ok(NixPackage {
                selector,
                attr,
                name: s(v, "pname").or_else(|| s(v, "name")).unwrap_or_default(),
                original_url: s(v, "originalUrl").or_else(|| s(v, "original_url")),
                store_paths: paths,
                active: v.get("active").and_then(Value::as_bool).unwrap_or(true),
            })
        })
        .collect()
}
pub fn parse_search(input: &str) -> Result<Vec<NixPackage>, NixError> {
    parse_profile(input)
}
#[derive(Clone, Debug)]
pub struct NixBackend {
    pub program: PathBuf,
}
impl NixBackend {
    const EXPERIMENTAL: &'static str = "nix-command flakes";
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, NixError> {
        let program = ExecutableResolver::from_path(path)
            .resolve(OsStr::new("nix"))
            .ok_or(NixError::MissingExecutable("nix"))?;
        Ok(Self { program })
    }
    pub fn from_paths(p: impl Into<PathBuf>) -> Self {
        Self { program: p.into() }
    }
    fn plan<const N: usize>(&self, args: [&str; N]) -> CommandPlan {
        let mut p = CommandPlan::new(self.program.clone())
            .with_backend(BackendId::Nix)
            .with_locale("C")
            .with_privilege(CommandPrivilege::User);
        p.args.push("--extra-experimental-features".into());
        p.args.push(Self::EXPERIMENTAL.into());
        p.args.extend(args.into_iter().map(OsString::from));
        p
    }
    pub fn profile_list_plan(&self) -> CommandPlan {
        self.plan(["profile", "list", "--json"])
    }
    pub fn search_plan(&self, q: &str) -> Result<CommandPlan, NixError> {
        if q.trim().len() < 2 {
            return Err(NixError::InvalidSelector);
        };
        Ok(self.plan(["search", "--json", "nixpkgs", q]))
    }
    pub fn details_plan(&self, selector: &str) -> Result<CommandPlan, NixError> {
        if selector.trim().is_empty() {
            return Err(NixError::InvalidSelector);
        }
        Ok(self.plan(["profile", "list", "--json"]))
    }
    pub fn execute(&self, plan: &CommandPlan) -> Result<crate::Output, NixError> {
        let mut command = std::process::Command::new(&plan.program);
        command.args(&plan.args);
        for variable in &plan.env_remove {
            command.env_remove(variable);
        }
        if let Some(locale) = &plan.locale {
            command.env("LC_ALL", locale);
        }
        let output = command.output().map_err(|error| {
            NixError::ExecutionFailed(format!("{}: {error}", plan.program.display()))
        })?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let diagnostic = stderr.to_ascii_lowercase();
        if !output.status.success()
            && diagnostic.contains("experimental")
            && diagnostic.contains("feature")
            && diagnostic.contains("disabled")
        {
            return Err(NixError::ExperimentalFeaturesDisabled);
        }
        Ok(crate::Output {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr,
        })
    }
    pub fn transaction(&self, op: WriteOperation) -> Result<TransactionPlan, NixError> {
        let (verb, ps) = match &op {
            WriteOperation::Install { packages } => ("install", packages),
            WriteOperation::Remove { packages } => ("remove", packages),
            WriteOperation::Upgrade { packages } => ("upgrade", packages),
            WriteOperation::Downgrade { .. } | WriteOperation::SystemUpgrade => {
                return Err(NixError::UnsupportedOperation);
            }
        };
        if ps.is_empty()
            || ps.iter().any(|p| {
                p.backend != BackendId::Nix
                    || p.kind != PackageKind::Nix
                    || p.scope != PackageScope::Profile
                    || p.native_key.as_str().trim().is_empty()
            })
        {
            return Err(NixError::InvalidSelector);
        };
        let mut c = self.plan(["profile", verb]);
        c.args
            .extend(ps.iter().map(|p| OsString::from(p.native_key.as_str())));
        Ok(TransactionPlan {
            backend: BackendId::Nix,
            kind: PackageKind::Nix,
            scope: PackageScope::Profile,
            operation: op.clone(),
            command: c,
            packages: ps.to_vec(),
        })
    }
}
pub fn identity(p: &NixPackage) -> PackageIdentity {
    PackageIdentity::new(
        BackendId::Nix,
        PackageKind::Nix,
        PackageScope::Profile,
        PackageId::new(p.selector.clone()).expect("parsed selector"),
    )
    .with_origin(p.original_url.clone().unwrap_or_else(|| p.attr.clone()))
    .with_display_name(p.name.clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_preserves_selector_and_paths() {
        let p = parse_profile(include_str!(
            "../../../../tests/package-managers/fixtures/nix/profile.json"
        ))
        .unwrap();
        assert_eq!(p[0].selector, "1");
        assert_eq!(p[0].attr, "hello");
        assert_eq!(p[0].store_paths.len(), 2);
        assert!(!p[1].active);
    }
    #[test]
    fn fixture_consumer_prints_records_and_exact_feature_enabled_plans() {
        let records = parse_profile(include_str!(
            "../../../../tests/package-managers/fixtures/nix/profile.json"
        ))
        .unwrap();
        let b = NixBackend::from_paths("/fake/nix");
        let package = identity(&records[0]);
        let plans = [
            (
                "profile",
                b.profile_list_plan(),
                vec!["profile", "list", "--json"],
            ),
            (
                "search",
                b.search_plan("hello;not-a-shell-command").unwrap(),
                vec!["search", "--json", "nixpkgs", "hello;not-a-shell-command"],
            ),
            (
                "details",
                b.details_plan("1").unwrap(),
                vec!["profile", "list", "--json"],
            ),
            (
                "install",
                b.transaction(WriteOperation::Install {
                    packages: vec![package.clone()],
                })
                .unwrap()
                .command,
                vec!["profile", "install", "1"],
            ),
            (
                "remove",
                b.transaction(WriteOperation::Remove {
                    packages: vec![package.clone()],
                })
                .unwrap()
                .command,
                vec!["profile", "remove", "1"],
            ),
            (
                "upgrade",
                b.transaction(WriteOperation::Upgrade {
                    packages: vec![package],
                })
                .unwrap()
                .command,
                vec!["profile", "upgrade", "1"],
            ),
        ];
        println!("records={records:?}");
        for (operation, plan, tail) in plans {
            let expected: Vec<OsString> = ["--extra-experimental-features", "nix-command flakes"]
                .into_iter()
                .chain(tail)
                .map(OsString::from)
                .collect();
            println!(
                "{operation} program={:?} argv={:?} privilege={:?}",
                plan.program, plan.args, plan.privilege
            );
            assert_eq!(plan.args, expected);
            assert_eq!(plan.privilege, CommandPrivilege::User);
            let output = execute_fixture(&b, plan, "success").unwrap();
            println!(
                "{operation} fixture status={} stdout={:?} stderr={:?}",
                output.status, output.stdout, output.stderr
            );
            let received: String = expected
                .iter()
                .map(|argument| format!("arg=<{}>\n", argument.to_string_lossy()))
                .collect();
            assert!(output.status.success());
            assert_eq!(output.stdout, format!("{received}locale=<C>\n"));
        }
    }
    #[test]
    fn search_and_malformed_are_guarded() {
        let b = NixBackend::from_paths("/nix");
        assert!(matches!(b.search_plan("x"), Err(NixError::InvalidSelector)));
        assert!(matches!(
            parse_profile("{}"),
            Err(NixError::MalformedJson(_))
        ));
    }

    #[test]
    fn maps_both_disabled_features_from_failed_command_without_config_mutation() {
        let backend = NixBackend::from_paths("/fake/nix");
        for mode in ["disabled-nix-command", "disabled-flakes"] {
            let result = execute_fixture(&backend, backend.profile_list_plan(), mode);
            println!("{mode} error={:?}", result.as_ref().err());
            assert!(matches!(
                result,
                Err(NixError::ExperimentalFeaturesDisabled)
            ));
        }
    }

    #[test]
    fn unrelated_experimental_failure_keeps_status_and_stderr() {
        // Given: a local command failing for an experimental package, not a disabled feature.
        let backend = NixBackend::from_paths("/fake/nix");
        let plan = backend.profile_list_plan();
        // When: the Nix execution boundary receives that failure.
        let output = execute_fixture(&backend, plan, "unrelated-failure")
            .expect("ordinary failure remains output");
        // Then: the caller can inspect the original failure, not a feature error.
        assert_eq!(output.status.code(), Some(7));
        println!(
            "unrelated-failure status={} stderr={:?}",
            output.status, output.stderr
        );
        assert_eq!(output.stderr, "experimental package build failed\n");
    }

    #[test]
    fn successful_command_with_disabled_feature_warning_is_not_an_error() {
        let backend = NixBackend::from_paths("/fake/nix");
        let output =
            execute_fixture(&backend, backend.profile_list_plan(), "successful-warning").unwrap();
        println!(
            "successful-warning status={} stderr={:?}",
            output.status, output.stderr
        );
        assert!(output.status.success());
        assert!(output.stderr.contains("disabled"));
    }

    #[test]
    fn execution_failure_keeps_program_and_cause() {
        let backend = NixBackend::from_paths("/missing/nix-review-fixture");
        let result = backend.execute(&backend.profile_list_plan());
        println!("spawn-failure error={:?}", result.as_ref().err());
        assert!(
            matches!(result, Err(NixError::ExecutionFailed(message)) if message.contains("/missing/nix-review-fixture"))
        );
    }

    fn execute_fixture(
        backend: &NixBackend,
        mut plan: CommandPlan,
        mode: &str,
    ) -> Result<crate::Output, NixError> {
        plan.program = PathBuf::from("/bin/sh");
        plan.args.splice(
            0..0,
            [
                OsString::from("-c"),
                OsString::from(include_str!(
                    "../../../../tests/package-managers/fixtures/nix/command.sh"
                )),
                OsString::from(mode),
            ],
        );
        backend.execute(&plan)
    }
}
