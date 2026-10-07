use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, PackageId, PackageKind,
    PackageScope, TransactionPlan, WriteOperation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApkPackage {
    pub name: String,
    pub epoch: Option<String>,
    pub version: String,
    pub revision: Option<String>,
    pub architecture: Option<String>,
    pub repository: Option<String>,
    pub tags: Vec<String>,
    pub world: bool,
    pub installed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApkUpdate {
    pub name: String,
    pub current: Option<String>,
    pub candidate: String,
    pub architecture: Option<String>,
    pub repository: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApkError {
    MalformedRecord { line: usize, reason: &'static str },
    InvalidPackageId,
    MissingExecutable(&'static str),
    UnsupportedOperation,
}

impl fmt::Display for ApkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedRecord { line, reason } => {
                write!(f, "invalid apk record at line {line}: {reason}")
            }
            Self::InvalidPackageId => f.write_str("package id must not be empty"),
            Self::MissingExecutable(name) => write!(f, "required executable not found: {name}"),
            Self::UnsupportedOperation => f.write_str("apk does not support this operation"),
        }
    }
}
impl std::error::Error for ApkError {}

fn parse_version(raw: &str) -> (Option<String>, String, Option<String>) {
    let (epoch, value) = raw
        .split_once(':')
        .map_or((None, raw), |(e, v)| (Some(e.to_owned()), v));
    match value.rsplit_once('-') {
        Some((version, revision)) if !version.is_empty() && !revision.is_empty() => {
            (epoch, version.to_owned(), Some(revision.to_owned()))
        }
        _ => (epoch, value.to_owned(), None),
    }
}

fn parse_line(line_no: usize, line: &str, installed: bool) -> Result<ApkPackage, ApkError> {
    let fields: Vec<_> = line.split('\t').collect();
    let compact = fields.len() < 2;
    let (name, raw_version) = if compact {
        split_compact_package(fields.first().copied().unwrap_or_default()).ok_or(
            ApkError::MalformedRecord {
                line: line_no,
                reason: "expected name and version",
            },
        )?
    } else {
        (fields[0].trim(), fields[1].trim())
    };
    if name.is_empty() || raw_version.is_empty() {
        return Err(ApkError::MalformedRecord {
            line: line_no,
            reason: "expected name and version",
        });
    }
    let (epoch, version, revision) = parse_version(raw_version);
    if version.trim().is_empty() || epoch.as_deref().is_some_and(str::is_empty) {
        return Err(ApkError::MalformedRecord {
            line: line_no,
            reason: "version is empty",
        });
    }
    let tags = fields
        .get(4)
        .map(|v| {
            v.split(',')
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let world = fields.get(5).is_some_and(|v| {
        *v == "1" || v.eq_ignore_ascii_case("world") || v.split(',').any(|tag| tag == "world")
    });
    Ok(ApkPackage {
        name: name.to_owned(),
        epoch,
        version,
        revision,
        architecture: fields
            .get(2)
            .filter(|v| !v.trim().is_empty())
            .map(|v| (*v).to_owned()),
        repository: fields
            .get(3)
            .filter(|v| !v.trim().is_empty())
            .map(|v| (*v).to_owned()),
        tags,
        world,
        installed,
    })
}

fn split_compact_package(value: &str) -> Option<(&str, &str)> {
    let value = value.trim();
    value.char_indices().rev().find_map(|(index, ch)| {
        (ch == '-' && value.get(index + 1..)?.chars().next()?.is_ascii_digit())
            .then(|| (&value[..index], &value[index + 1..]))
    })
}

pub fn parse_search(input: &str) -> Result<Vec<ApkPackage>, ApkError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(n, line)| parse_line(n + 1, line, false))
        .collect()
}

pub fn parse_query(input: &str) -> Result<Vec<ApkPackage>, ApkError> {
    parse_search(input)
}

pub fn parse_installed(input: &str) -> Result<Vec<ApkPackage>, ApkError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(n, line)| parse_line(n + 1, line, true))
        .collect()
}

pub fn parse_updates(input: &str) -> Result<Vec<ApkUpdate>, ApkError> {
    parse_search(input)?
        .into_iter()
        .map(|p| {
            Ok(ApkUpdate {
                name: p.name,
                current: None,
                candidate: p.version,
                architecture: p.architecture,
                repository: p.repository,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct ApkBackend {
    pub program: PathBuf,
}

impl ApkBackend {
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, ApkError> {
        let program = ExecutableResolver::from_path(path)
            .resolve(OsStr::new("apk"))
            .ok_or(ApkError::MissingExecutable("apk"))?;
        Ok(Self { program })
    }
    pub fn from_paths(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }
    fn read<const N: usize>(&self, args: [&str; N]) -> CommandPlan {
        let mut p = CommandPlan::new(self.program.clone())
            .with_backend(BackendId::Apk)
            .with_locale("C")
            .with_privilege(CommandPrivilege::User);
        p.args.extend(args.into_iter().map(OsString::from));
        p
    }
    fn elevated<const N: usize>(&self, args: [&str; N]) -> CommandPlan {
        self.read(args).with_privilege(CommandPrivilege::Elevated)
    }
    pub fn search_plan(&self, query: &str) -> CommandPlan {
        self.read(["search", "--no-cache", query])
    }
    pub fn query_plan(&self, query: &str) -> CommandPlan {
        self.read(["query", "--no-cache", query])
    }
    pub fn list_plan(&self) -> CommandPlan {
        self.read(["search", "--no-cache", "*"])
    }
    pub fn installed_plan(&self) -> CommandPlan {
        self.read(["info", "--installed"])
    }
    pub fn details_plan(&self, package: &PackageId) -> Result<CommandPlan, ApkError> {
        if package.as_str().trim().is_empty() {
            return Err(ApkError::InvalidPackageId);
        }
        Ok(self.read(["info", package.as_str()]))
    }
    pub fn updates_plan(&self) -> CommandPlan {
        self.read(["list", "--upgradable"])
    }
    pub fn update_catalog_plan(&self) -> CommandPlan {
        self.elevated(["update"])
    }
    pub fn system_upgrade_plan(&self) -> CommandPlan {
        self.elevated(["upgrade"])
    }
    pub fn transaction(&self, operation: WriteOperation) -> Result<TransactionPlan, ApkError> {
        let operation_for_plan = operation.clone();
        let (verb, packages) = match &operation {
            WriteOperation::Install { packages } => ("add", packages),
            WriteOperation::Remove { packages } => ("del", packages),
            WriteOperation::SystemUpgrade => {
                return Ok(TransactionPlan {
                    backend: BackendId::Apk,
                    kind: PackageKind::System,
                    scope: PackageScope::System,
                    operation,
                    command: self.system_upgrade_plan(),
                    packages: Vec::new(),
                });
            }
            _ => return Err(ApkError::UnsupportedOperation),
        };
        if packages.is_empty()
            || packages
                .iter()
                .any(|p| p.native_key.as_str().trim().is_empty())
        {
            return Err(ApkError::InvalidPackageId);
        }
        let mut command = self.elevated([verb]);
        command.args.extend(
            packages
                .iter()
                .map(|p| OsString::from(p.native_key.as_str())),
        );
        Ok(TransactionPlan {
            backend: BackendId::Apk,
            kind: PackageKind::System,
            scope: PackageScope::System,
            operation: operation_for_plan,
            command,
            packages: packages.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageIdentity, WriteOperation};

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::new(
            BackendId::Apk,
            PackageKind::System,
            PackageScope::System,
            PackageId::new(name).unwrap(),
        )
    }

    #[test]
    fn parses_fixture_and_preserves_hyphenated_names() {
        let records = parse_installed(include_str!(
            "../../../../tests/package-managers/fixtures/apk/installed.tsv"
        ))
        .unwrap();
        assert_eq!(records[0].name, "libfoo-bar");
        assert_eq!(records[0].epoch.as_deref(), Some("2"));
        assert_eq!(records[0].revision.as_deref(), Some("r3"));
        assert!(records[0].world);
        assert_eq!(records[0].tags, ["busybox", "world"]);
    }

    #[test]
    fn fixture_consumer_prints_records_and_exact_plans() {
        let catalog = parse_search(include_str!(
            "../../../../tests/package-managers/fixtures/apk/search.tsv"
        ))
        .unwrap();
        let updates = parse_updates(include_str!(
            "../../../../tests/package-managers/fixtures/apk/updates.tsv"
        ))
        .unwrap();
        let backend = ApkBackend::from_paths("/fake/usr/bin/apk");
        let transaction = backend
            .transaction(WriteOperation::Install {
                packages: vec![identity("pkg;touch /tmp/nope")],
            })
            .unwrap();
        println!("catalog={catalog:?}");
        println!("updates={updates:?}");
        println!("install_plan={transaction:?}");
        assert_eq!(transaction.command.args, ["add", "pkg;touch /tmp/nope"]);
        assert_eq!(transaction.command.privilege, CommandPrivilege::Elevated);
        assert_eq!(backend.update_catalog_plan().args, ["update"]);
        assert_eq!(backend.system_upgrade_plan().args, ["upgrade"]);
    }

    #[test]
    fn malformed_and_empty_ids_fail_without_shell_interpretation() {
        assert!(matches!(
            parse_search("broken\n"),
            Err(ApkError::MalformedRecord { .. })
        ));
        let backend = ApkBackend::from_paths("/apk");
        assert!(matches!(
            PackageId::new(" "),
            Err(crate::BackendError::InvalidPackageId)
        ));
        assert!(
            backend
                .details_plan(&PackageId::new("pkg").unwrap())
                .is_ok()
        );
    }
}
