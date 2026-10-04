use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use serde_json::Value;

use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, PackageIdentity, PackageKind,
    PackageScope, TransactionPlan, WriteOperation,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrewKind {
    Formula,
    Cask,
}

impl BrewKind {
    pub const fn package_kind(self) -> PackageKind {
        match self {
            Self::Formula => PackageKind::BrewFormula,
            Self::Cask => PackageKind::BrewCask,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrewPackage {
    pub token: String,
    pub full_name: String,
    pub tap: Option<String>,
    pub version: Option<String>,
    pub installed: bool,
    pub kind: BrewKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrewUpdate {
    pub token: String,
    pub full_name: String,
    pub tap: Option<String>,
    pub current: Option<String>,
    pub candidate: String,
    pub kind: BrewKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrewError {
    MalformedJson(String),
    MissingField { record: usize, field: &'static str },
    InvalidPackageId,
    MissingExecutable(&'static str),
    UnsupportedCaskOnLinux,
    UnsupportedOperation,
}

impl fmt::Display for BrewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedJson(e) => write!(f, "invalid brew JSON: {e}"),
            Self::MissingField { record, field } => {
                write!(f, "brew record {record} is missing {field}")
            }
            Self::InvalidPackageId => f.write_str("package id must not be empty"),
            Self::MissingExecutable(name) => write!(f, "required executable not found: {name}"),
            Self::UnsupportedCaskOnLinux => {
                f.write_str("brew casks are unavailable on this Linux host")
            }
            Self::UnsupportedOperation => f.write_str("brew does not support this operation"),
        }
    }
}
impl std::error::Error for BrewError {}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

fn parse_records(
    input: &str,
    kind: BrewKind,
    installed: bool,
) -> Result<Vec<BrewPackage>, BrewError> {
    let root: Value =
        serde_json::from_str(input).map_err(|e| BrewError::MalformedJson(e.to_string()))?;
    let records = root
        .get("formulae")
        .or_else(|| root.get("casks"))
        .and_then(Value::as_array)
        .ok_or_else(|| BrewError::MalformedJson("expected formulae or casks array".to_owned()))?;
    records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let token = string_field(record, "name")
                .or_else(|| string_field(record, "token"))
                .ok_or(BrewError::MissingField {
                    record: index + 1,
                    field: "name/token",
                })?;
            let full_name = string_field(record, "full_name").unwrap_or_else(|| token.clone());
            Ok(BrewPackage {
                token,
                full_name,
                tap: string_field(record, "tap"),
                version: string_field(record, "version").or_else(|| {
                    record
                        .get("versions")
                        .and_then(|v| v.get("stable"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                }),
                installed,
                kind,
            })
        })
        .collect()
}

pub fn parse_formulae(input: &str) -> Result<Vec<BrewPackage>, BrewError> {
    parse_records(input, BrewKind::Formula, false)
}
pub fn parse_casks(input: &str) -> Result<Vec<BrewPackage>, BrewError> {
    parse_records(input, BrewKind::Cask, false)
}
pub fn parse_info(input: &str, kind: BrewKind) -> Result<Vec<BrewPackage>, BrewError> {
    parse_records(input, kind, true)
}

pub fn parse_outdated(input: &str) -> Result<Vec<BrewUpdate>, BrewError> {
    let root: Value =
        serde_json::from_str(input).map_err(|e| BrewError::MalformedJson(e.to_string()))?;
    let records = root
        .get("formulae")
        .or_else(|| root.get("casks"))
        .and_then(Value::as_array)
        .ok_or_else(|| BrewError::MalformedJson("expected formulae or casks array".to_owned()))?;
    records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let token = string_field(record, "name")
                .or_else(|| string_field(record, "token"))
                .ok_or(BrewError::MissingField {
                    record: index + 1,
                    field: "name/token",
                })?;
            let candidate = string_field(record, "latest_version")
                .or_else(|| string_field(record, "latest"))
                .or_else(|| string_field(record, "current_version"))
                .ok_or(BrewError::MissingField {
                    record: index + 1,
                    field: "latest_version",
                })?;
            Ok(BrewUpdate {
                full_name: string_field(record, "full_name").unwrap_or_else(|| token.clone()),
                token,
                tap: string_field(record, "tap"),
                current: string_field(record, "installed_versions")
                    .or_else(|| {
                        record
                            .get("installed_versions")?
                            .as_array()?
                            .first()?
                            .as_str()
                            .map(str::to_owned)
                    })
                    .or_else(|| string_field(record, "current_version")),
                candidate,
                kind: if root.get("casks").is_some() {
                    BrewKind::Cask
                } else {
                    BrewKind::Formula
                },
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct BrewBackend {
    pub program: PathBuf,
}

impl BrewBackend {
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, BrewError> {
        let program = ExecutableResolver::from_path(path)
            .resolve(OsStr::new("brew"))
            .ok_or(BrewError::MissingExecutable("brew"))?;
        Ok(Self { program })
    }
    pub fn from_paths(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }
    fn read<const N: usize>(&self, args: [&str; N]) -> CommandPlan {
        let mut plan = CommandPlan::new(self.program.clone())
            .with_backend(BackendId::Brew)
            .with_locale("C")
            .with_privilege(CommandPrivilege::User);
        plan.args.extend(args.into_iter().map(OsString::from));
        plan
    }
    pub fn formulae_plan(&self) -> CommandPlan {
        self.read(["formulae"])
    }
    pub fn casks_plan(&self) -> CommandPlan {
        self.read(["casks"])
    }
    pub fn search_plan(&self, query: &str) -> Result<CommandPlan, BrewError> {
        if query.trim().is_empty() {
            return Err(BrewError::InvalidPackageId);
        }
        Ok(self.read(["search", "--formula", "--", query]))
    }
    pub fn info_plan(&self, token: &str, kind: BrewKind) -> Result<CommandPlan, BrewError> {
        if token.trim().is_empty() {
            return Err(BrewError::InvalidPackageId);
        }
        Ok(self.read([
            "info",
            "--json=v2",
            if kind == BrewKind::Cask {
                "--cask"
            } else {
                "--formula"
            },
            token,
        ]))
    }
    pub fn outdated_plan(&self) -> CommandPlan {
        self.read(["outdated", "--json=v2"])
    }
    pub fn transaction(&self, operation: WriteOperation) -> Result<TransactionPlan, BrewError> {
        let (verb, packages) = match &operation {
            WriteOperation::Install { packages } => ("install", packages),
            WriteOperation::Remove { packages } => ("uninstall", packages),
            WriteOperation::Upgrade { packages } => ("upgrade", packages),
            _ => return Err(BrewError::UnsupportedOperation),
        };
        if packages.is_empty()
            || packages
                .iter()
                .any(|p| p.native_key.as_str().trim().is_empty())
        {
            return Err(BrewError::InvalidPackageId);
        }
        let kind = packages[0].kind;
        if packages.iter().any(|p| {
            p.backend != BackendId::Brew || p.scope != PackageScope::Profile || p.kind != kind
        }) {
            return Err(BrewError::InvalidPackageId);
        }
        if kind == PackageKind::BrewCask {
            return Err(BrewError::UnsupportedCaskOnLinux);
        }
        let mut command = self.read([verb]);
        command.args.extend(
            packages
                .iter()
                .map(|p| OsString::from(p.native_key.as_str())),
        );
        Ok(TransactionPlan {
            backend: BackendId::Brew,
            kind,
            scope: PackageScope::Profile,
            operation: operation.clone(),
            command,
            packages: packages.to_vec(),
        })
    }
}

pub fn identity(package: &BrewPackage, scope: PackageScope) -> PackageIdentity {
    PackageIdentity::new(
        BackendId::Brew,
        package.kind.package_kind(),
        scope,
        crate::PackageId::new(package.token.clone()).expect("parsed brew token is non-empty"),
    )
    .with_origin(
        package
            .tap
            .clone()
            .unwrap_or_else(|| package.full_name.clone()),
    )
    .with_display_name(package.full_name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageId;

    #[test]
    fn fixture_parser_preserves_formula_cask_identity() {
        let formulas = parse_formulae(include_str!(
            "../../../../tests/package-managers/fixtures/brew/formulae.json"
        ))
        .unwrap();
        let casks = parse_casks(include_str!(
            "../../../../tests/package-managers/fixtures/brew/casks.json"
        ))
        .unwrap();
        assert_eq!(formulas[0].token, casks[0].token);
        assert_ne!(
            identity(&formulas[0], PackageScope::Profile).kind,
            identity(&casks[0], PackageScope::Profile).kind
        );
        assert_eq!(formulas[0].full_name, "homebrew/core/hello");
        let details = parse_info(
            include_str!("../../../../tests/package-managers/fixtures/brew/info.json"),
            BrewKind::Formula,
        )
        .unwrap();
        let updates = parse_outdated(include_str!(
            "../../../../tests/package-managers/fixtures/brew/outdated.json"
        ))
        .unwrap();
        assert!(details[0].installed);
        assert_eq!(updates[0].candidate, "2.12.1");
    }

    #[test]
    fn plans_are_user_only_and_structured() {
        let backend = BrewBackend::from_paths("/fake/brew");
        let package = PackageIdentity::new(
            BackendId::Brew,
            PackageKind::BrewFormula,
            PackageScope::Profile,
            PackageId::new("hello;touch /tmp/nope").unwrap(),
        );
        let plan = backend
            .transaction(WriteOperation::Install {
                packages: vec![package],
            })
            .unwrap();
        assert_eq!(plan.command.args, ["install", "hello;touch /tmp/nope"]);
        assert_eq!(plan.command.privilege, CommandPrivilege::User);
        assert!(!plan.command.args.iter().any(|a| a == "sudo"));
    }

    #[test]
    fn malformed_and_linux_cask_are_explicit() {
        assert!(matches!(
            parse_formulae("{}"),
            Err(BrewError::MalformedJson(_))
        ));
        let backend = BrewBackend::from_paths("/fake/brew");
        let package = PackageIdentity::new(
            BackendId::Brew,
            PackageKind::BrewCask,
            PackageScope::Profile,
            PackageId::new("hello").unwrap(),
        );
        assert_eq!(
            backend.transaction(WriteOperation::Install {
                packages: vec![package]
            }),
            Err(BrewError::UnsupportedCaskOnLinux)
        );
    }
}
