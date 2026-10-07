use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, PackageId, PackageKind,
    PackageScope, TransactionPlan, WriteOperation,
};
use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XbpsPackage {
    pub name: String,
    pub version: String,
    pub revision: Option<String>,
    pub architecture: Option<String>,
    pub repository: Option<String>,
    pub installed: bool,
    pub held: bool,
    pub virtual_package: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XbpsUpdate {
    pub name: String,
    pub current: Option<String>,
    pub candidate: String,
    pub architecture: Option<String>,
    pub repository: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XbpsError {
    MalformedRecord { line: usize, reason: &'static str },
    MalformedPkgver(String),
    InvalidPackageId,
    MissingExecutable(&'static str),
    UnsupportedOperation,
}
impl fmt::Display for XbpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedRecord { line, reason } => {
                write!(f, "invalid XBPS record at line {line}: {reason}")
            }
            Self::MalformedPkgver(v) => write!(f, "invalid XBPS package version: {v}"),
            Self::InvalidPackageId => f.write_str("package id must not be empty"),
            Self::MissingExecutable(n) => write!(f, "required executable not found: {n}"),
            Self::UnsupportedOperation => f.write_str("XBPS does not support this operation"),
        }
    }
}
impl std::error::Error for XbpsError {}

pub fn parse_pkgver(value: &str) -> Result<(String, String, Option<String>), XbpsError> {
    let value = value.trim();
    let (base, revision) = match value.rsplit_once('_') {
        Some((b, r)) if !r.is_empty() && r.bytes().all(|c| c.is_ascii_digit()) => {
            (b, Some(r.to_owned()))
        }
        _ => (value, None),
    };
    let boundary = base
        .char_indices()
        .filter(|(_, c)| *c == '-')
        .rev()
        .find(|(i, _)| {
            base.get(*i + 1..).is_some_and(|s| {
                let first = s.chars().next();
                let has_digit = s.chars().any(|c| c.is_ascii_digit());
                has_digit
                    && first.is_some_and(|c| {
                        c.is_ascii_digit()
                            || c.is_ascii_uppercase()
                            || c == 'v'
                            || c.is_ascii_lowercase()
                    })
            })
        })
        .map(|(i, _)| i);
    let Some(i) = boundary else {
        return Err(XbpsError::MalformedPkgver(value.to_owned()));
    };
    let (name, version) = base.split_at(i);
    let version = &version[1..];
    if name.is_empty() || version.is_empty() {
        return Err(XbpsError::MalformedPkgver(value.to_owned()));
    }
    Ok((name.to_owned(), version.to_owned(), revision))
}

fn parse_record(line_no: usize, line: &str, installed: bool) -> Result<XbpsPackage, XbpsError> {
    let fields: Vec<_> = line.split('\t').collect();
    let tabular = fields.len() >= 2;
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let package_token = if tabular {
        fields.first().copied().unwrap_or_default()
    } else if tokens.first().is_some_and(|token| token.starts_with('[')) {
        tokens.get(1).copied().unwrap_or_default()
    } else {
        tokens
            .iter()
            .find(|token| parse_pkgver(token).is_ok())
            .copied()
            .unwrap_or_default()
    };
    if package_token.trim().is_empty() {
        return Err(XbpsError::MalformedRecord {
            line: line_no,
            reason: "expected package version",
        });
    }
    let (name, version, revision) = parse_pkgver(package_token)?;
    Ok(XbpsPackage {
        name,
        version,
        revision,
        repository: fields
            .get(1)
            .filter(|v| !v.is_empty() && tabular)
            .map(|v| v.trim().to_owned()),
        architecture: fields
            .get(2)
            .filter(|v| !v.is_empty() && tabular)
            .map(|v| (*v).to_owned()),
        installed,
        held: fields.get(3).is_some_and(|v| *v == "hold"),
        virtual_package: fields.get(3).is_some_and(|v| *v == "virtual"),
    })
}
pub fn parse_search(input: &str) -> Result<Vec<XbpsPackage>, XbpsError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            !l.trim().is_empty()
                && !l.trim_start().starts_with('#')
                && !l.trim_start().starts_with("WARNING:")
        })
        .map(|(i, l)| parse_record(i + 1, l, false))
        .collect()
}
pub fn parse_installed(input: &str) -> Result<Vec<XbpsPackage>, XbpsError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|(i, l)| parse_record(i + 1, l, true))
        .collect()
}
pub fn parse_updates(input: &str) -> Result<Vec<XbpsUpdate>, XbpsError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("WARNING:"))
        .map(|(i, l)| {
            let fields: Vec<_> = if l.contains('\t') {
                l.split('\t').collect()
            } else {
                l.split_whitespace().collect()
            };
            if fields.len() < 3 {
                return Err(XbpsError::MalformedRecord {
                    line: i + 1,
                    reason: "expected name, current and candidate",
                });
            }
            if matches!(fields[1], "install" | "update") {
                let (name, candidate, _) = parse_pkgver(fields[0])?;
                return Ok(XbpsUpdate {
                    name,
                    current: None,
                    candidate,
                    repository: fields.get(3).map(|v| (*v).to_owned()),
                    architecture: fields.get(2).map(|v| (*v).to_owned()),
                });
            }
            let (name, current, _) = parse_pkgver(fields[0])?;
            let (_, candidate, _) = parse_pkgver(fields[1])?;
            Ok(XbpsUpdate {
                name,
                current: Some(current),
                candidate,
                repository: fields.get(2).map(|v| (*v).to_owned()),
                architecture: fields.get(3).map(|v| (*v).to_owned()),
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct XbpsBackend {
    pub query: PathBuf,
    pub install: PathBuf,
    pub remove: PathBuf,
}
impl XbpsBackend {
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, XbpsError> {
        let r = ExecutableResolver::from_path(path);
        Ok(Self {
            query: r
                .resolve(OsStr::new("xbps-query"))
                .ok_or(XbpsError::MissingExecutable("xbps-query"))?,
            install: r
                .resolve(OsStr::new("xbps-install"))
                .ok_or(XbpsError::MissingExecutable("xbps-install"))?,
            remove: r
                .resolve(OsStr::new("xbps-remove"))
                .ok_or(XbpsError::MissingExecutable("xbps-remove"))?,
        })
    }
    pub fn from_paths(
        query: impl Into<PathBuf>,
        install: impl Into<PathBuf>,
        remove: impl Into<PathBuf>,
    ) -> Self {
        Self {
            query: query.into(),
            install: install.into(),
            remove: remove.into(),
        }
    }
    fn read<const N: usize>(&self, args: [&str; N]) -> CommandPlan {
        let mut p = CommandPlan::new(self.query.clone())
            .with_backend(BackendId::Xbps)
            .with_locale("C")
            .with_privilege(CommandPrivilege::User);
        p.args.extend(args.into_iter().map(OsString::from));
        p
    }
    pub fn search_plan(&self, query: &str) -> CommandPlan {
        self.read(["-Rs", query])
    }
    pub fn list_plan(&self) -> CommandPlan {
        self.read(["-Rs", "."])
    }
    pub fn installed_plan(&self) -> CommandPlan {
        self.read(["-l"])
    }
    pub fn details_plan(&self, package: &PackageId) -> Result<CommandPlan, XbpsError> {
        if package.as_str().trim().is_empty() {
            return Err(XbpsError::InvalidPackageId);
        }
        Ok(self.read(["-S", package.as_str()]))
    }
    pub fn updates_plan(&self) -> CommandPlan {
        let mut p = CommandPlan::new(self.install.clone())
            .with_backend(BackendId::Xbps)
            .with_locale("C")
            .with_privilege(CommandPrivilege::User);
        p.args.extend(["-u", "-n"].into_iter().map(OsString::from));
        p
    }
    pub fn sync_plan(&self) -> CommandPlan {
        let mut p = CommandPlan::new(self.install.clone())
            .with_backend(BackendId::Xbps)
            .with_locale("C")
            .with_privilege(CommandPrivilege::Elevated);
        p.args.push("-S".into());
        p
    }
    pub fn system_upgrade_plan(&self) -> CommandPlan {
        let mut p = CommandPlan::new(self.install.clone())
            .with_backend(BackendId::Xbps)
            .with_locale("C")
            .with_privilege(CommandPrivilege::Elevated);
        p.args.extend(["-Su"].into_iter().map(OsString::from));
        p
    }
    pub fn transaction(&self, operation: WriteOperation) -> Result<TransactionPlan, XbpsError> {
        let operation_for_plan = operation.clone();
        let (program, verb, packages) = match &operation {
            WriteOperation::Install { packages } => (self.install.clone(), "", packages),
            WriteOperation::Remove { packages } => (self.remove.clone(), "-y", packages),
            WriteOperation::SystemUpgrade => {
                return Ok(TransactionPlan {
                    backend: BackendId::Xbps,
                    kind: PackageKind::System,
                    scope: PackageScope::System,
                    operation,
                    command: self.system_upgrade_plan(),
                    packages: Vec::new(),
                });
            }
            _ => return Err(XbpsError::UnsupportedOperation),
        };
        if packages.is_empty()
            || packages
                .iter()
                .any(|p| p.native_key.as_str().trim().is_empty())
        {
            return Err(XbpsError::InvalidPackageId);
        }
        let mut c = CommandPlan::new(program)
            .with_backend(BackendId::Xbps)
            .with_locale("C")
            .with_privilege(CommandPrivilege::Elevated);
        if !verb.is_empty() {
            c.args.push(OsString::from(verb));
        }
        c.args.extend(
            packages
                .iter()
                .map(|p| OsString::from(p.native_key.as_str())),
        );
        Ok(TransactionPlan {
            backend: BackendId::Xbps,
            kind: PackageKind::System,
            scope: PackageScope::System,
            operation: operation_for_plan,
            command: c,
            packages: packages.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendId, PackageIdentity, WriteOperation};
    fn id(name: &str) -> PackageIdentity {
        PackageIdentity::new(
            BackendId::Xbps,
            PackageKind::System,
            PackageScope::System,
            crate::NativePackageKey::new(name).unwrap(),
        )
    }
    #[test]
    fn pkgver_keeps_hyphenated_name_and_revision() {
        assert_eq!(
            parse_pkgver("lib-foo-bar-1.2_3").unwrap(),
            ("lib-foo-bar".into(), "1.2".into(), Some("3".into()))
        );
        assert!(parse_pkgver("bad-name").is_err());
    }
    #[test]
    fn parses_fixture_and_plans() {
        let p = parse_search(include_str!(
            "../../../../tests/package-managers/fixtures/xbps/search.tsv"
        ))
        .unwrap();
        assert_eq!(p[0].name, "lib-foo");
        assert_eq!(p[0].revision.as_deref(), Some("2"));
        let b = XbpsBackend::from_paths("/xbps-query", "/xbps-install", "/xbps-remove");
        assert_eq!(b.search_plan("foo").args, ["-Rs", "foo"]);
        assert_eq!(b.sync_plan().args, ["-S"]);
        assert_eq!(b.system_upgrade_plan().args, ["-Su"]);
        let t = b
            .transaction(WriteOperation::Install {
                packages: vec![id("foo;touch /tmp/x")],
            })
            .unwrap();
        assert_eq!(t.command.program, std::path::Path::new("/xbps-install"));
        assert_eq!(t.command.args, ["foo;touch /tmp/x"]);
        assert_eq!(t.command.privilege, CommandPrivilege::Elevated);
    }

    #[test]
    fn parses_installed_and_updates_fixtures_and_prints_plans() {
        let installed =
            parse_installed("# xbps-query header\n\nlib-foo-1.2_3\trepo\tx86_64\n").unwrap();
        assert_eq!(installed.len(), 1);
        assert!(installed[0].installed);
        let updates = parse_updates(include_str!(
            "../../../../tests/package-managers/fixtures/xbps/updates.tsv"
        ))
        .unwrap();
        assert_eq!(updates[0].name, "lib-foo");
        assert_eq!(updates[0].current.as_deref(), Some("1.2"));
        assert_eq!(updates[0].candidate, "1.3");
        let install =
            parse_updates("libicu78-78.3_1 install x86_64 https://repo.example\n").unwrap();
        assert_eq!(install[0].name, "libicu78");
        assert_eq!(install[0].current, None);
        assert_eq!(install[0].candidate, "78.3");
        println!("installed={installed:?} updates={updates:?}");
    }

    #[test]
    fn search_ignores_provider_warnings_before_records() {
        let packages = parse_search(
            "WARNING: tzdata-2025a_1: invalid provides: py3:tzdata-2025a\n[-] tzdata-2026e_1 Time zone data\n",
        )
        .expect("parse search output with an XBPS warning");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "tzdata");
        assert_eq!(packages[0].version, "2026e");
    }
}
