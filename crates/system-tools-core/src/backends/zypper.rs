use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, PackageId, PackageKind,
    PackageScope, TransactionPlan, WriteOperation,
};
use quick_xml::{Reader, events::Event};
use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZypperPackage {
    pub name: String,
    pub version: String,
    pub architecture: Option<String>,
    pub repository: Option<String>,
    pub installed: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZypperUpdate {
    pub name: String,
    pub current: Option<String>,
    pub candidate: String,
    pub architecture: Option<String>,
    pub repository: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ZypperError {
    MalformedXml(String),
    MissingField { record: usize, field: &'static str },
    InvalidPackageId,
    MissingExecutable(&'static str),
    UnsupportedOperation,
}
impl fmt::Display for ZypperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedXml(e) => write!(f, "invalid zypper XML: {e}"),
            Self::MissingField { record, field } => {
                write!(f, "zypper record {record} is missing {field}")
            }
            Self::InvalidPackageId => f.write_str("package id must not be empty"),
            Self::MissingExecutable(n) => write!(f, "required executable not found: {n}"),
            Self::UnsupportedOperation => f.write_str("zypper does not support this operation"),
        }
    }
}
impl std::error::Error for ZypperError {}
#[derive(Default)]
struct Record {
    kind: Option<String>,
    name: Option<String>,
    version: Option<String>,
    arch: Option<String>,
    repo: Option<String>,
    installed: bool,
    current: Option<String>,
    candidate: Option<String>,
}
fn lname(v: &[u8]) -> String {
    std::str::from_utf8(v)
        .unwrap_or_default()
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}
fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| lname(a.key.as_ref()) == key)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}
fn records(input: &str) -> Result<Vec<Record>, ZypperError> {
    let mut r = Reader::from_str(input);
    r.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut cur: Option<Record> = None;
    let mut field = None;
    let mut depth = 0usize;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                let t = lname(e.name().as_ref());
                if t == "solvable" {
                    cur = Some(Record {
                        kind: attr(&e, "type").or_else(|| attr(&e, "kind")),
                        name: attr(&e, "name"),
                        version: attr(&e, "version"),
                        arch: attr(&e, "arch"),
                        repo: attr(&e, "repo").or_else(|| attr(&e, "repository")),
                        installed: attr(&e, "status")
                            .is_some_and(|s| s.eq_ignore_ascii_case("installed")),
                        current: attr(&e, "current"),
                        candidate: attr(&e, "candidate"),
                    })
                } else if cur.is_some() {
                    field = Some(t)
                }
            }
            Ok(Event::Empty(e)) => {
                let t = lname(e.name().as_ref());
                if t == "solvable" {
                    out.push(Record {
                        kind: attr(&e, "type").or_else(|| attr(&e, "kind")),
                        name: attr(&e, "name"),
                        version: attr(&e, "version"),
                        arch: attr(&e, "arch"),
                        repo: attr(&e, "repo").or_else(|| attr(&e, "repository")),
                        installed: attr(&e, "status")
                            .is_some_and(|s| s.eq_ignore_ascii_case("installed")),
                        current: attr(&e, "current"),
                        candidate: attr(&e, "candidate"),
                    })
                } else if let Some(x) = cur.as_mut()
                    && matches!(t.as_str(), "repository" | "repo" | "alias")
                {
                    x.repo = attr(&e, "alias")
                        .or_else(|| attr(&e, "name"))
                        .or_else(|| attr(&e, "repo"));
                }
            }
            Ok(Event::Text(e)) => {
                if let (Some(x), Some(f)) = (cur.as_mut(), field.as_deref()) {
                    let v = e
                        .decode()
                        .map_err(|e| ZypperError::MalformedXml(e.to_string()))?
                        .into_owned();
                    append_field(x, f, &v);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if let (Some(x), Some(f)) = (cur.as_mut(), field.as_deref()) {
                    let reference = std::str::from_utf8(&e)
                        .map_err(|e| ZypperError::MalformedXml(e.to_string()))?;
                    let escaped = format!("&{reference};");
                    let value = quick_xml::escape::unescape(&escaped)
                        .map_err(|e| ZypperError::MalformedXml(e.to_string()))?;
                    append_field(x, f, &value);
                }
            }
            Ok(Event::End(e)) => {
                if lname(e.name().as_ref()) == "solvable"
                    && let Some(x) = cur.take()
                {
                    out.push(x)
                }
                depth = depth.checked_sub(1).ok_or_else(|| {
                    ZypperError::MalformedXml("unexpected closing element".into())
                })?;
                field = None
            }
            Ok(Event::Eof) => {
                if depth != 0 {
                    return Err(ZypperError::MalformedXml("truncated XML document".into()));
                }
                break;
            }
            Err(e) => return Err(ZypperError::MalformedXml(e.to_string())),
            _ => {}
        }
    }
    Ok(out)
}
fn append_field(record: &mut Record, field: &str, value: &str) {
    match field {
        "name" => record.name.get_or_insert_with(String::new).push_str(value),
        "version" | "evr" => record
            .version
            .get_or_insert_with(String::new)
            .push_str(value),
        "arch" | "architecture" => record.arch.get_or_insert_with(String::new).push_str(value),
        "repository" | "repo" | "alias" => {
            record.repo.get_or_insert_with(String::new).push_str(value)
        }
        "status" => record.installed = value.eq_ignore_ascii_case("installed"),
        "current" => record
            .current
            .get_or_insert_with(String::new)
            .push_str(value),
        "candidate" => record
            .candidate
            .get_or_insert_with(String::new)
            .push_str(value),
        "type" | "kind" => record.kind.get_or_insert_with(String::new).push_str(value),
        _ => {}
    }
}
fn package(r: &Record) -> bool {
    r.kind
        .as_deref()
        .is_none_or(|k| k.eq_ignore_ascii_case("package"))
}
pub fn parse_search_xml(i: &str) -> Result<Vec<ZypperPackage>, ZypperError> {
    records(i)?
        .into_iter()
        .enumerate()
        .filter(|(_, r)| package(r))
        .map(|(n, r)| {
            Ok(ZypperPackage {
                name: r
                    .name
                    .filter(|x| !x.trim().is_empty())
                    .ok_or(ZypperError::MissingField {
                        record: n + 1,
                        field: "name",
                    })?,
                version: r.version.filter(|x| !x.trim().is_empty()).ok_or(
                    ZypperError::MissingField {
                        record: n + 1,
                        field: "version",
                    },
                )?,
                architecture: r.arch,
                repository: r.repo,
                installed: r.installed,
            })
        })
        .collect()
}
pub fn parse_info_xml(i: &str) -> Result<Vec<ZypperPackage>, ZypperError> {
    parse_search_xml(i)
}
pub fn parse_updates_xml(i: &str) -> Result<Vec<ZypperUpdate>, ZypperError> {
    records(i)?
        .into_iter()
        .enumerate()
        .filter(|(_, r)| package(r))
        .map(|(n, r)| {
            Ok(ZypperUpdate {
                name: r.name.ok_or(ZypperError::MissingField {
                    record: n + 1,
                    field: "name",
                })?,
                current: r.current,
                candidate: r.candidate.or(r.version).ok_or(ZypperError::MissingField {
                    record: n + 1,
                    field: "candidate",
                })?,
                architecture: r.arch,
                repository: r.repo,
            })
        })
        .collect()
}
#[derive(Clone, Debug)]
pub struct ZypperBackend {
    pub program: PathBuf,
}
impl ZypperBackend {
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, ZypperError> {
        Ok(Self {
            program: ExecutableResolver::from_path(path)
                .resolve(OsStr::new("zypper"))
                .ok_or(ZypperError::MissingExecutable("zypper"))?,
        })
    }
    pub fn from_paths(p: impl Into<PathBuf>) -> Self {
        Self { program: p.into() }
    }
    fn plan<const N: usize>(&self, a: [&str; N], p: CommandPrivilege) -> CommandPlan {
        let mut c = CommandPlan::new(self.program.clone())
            .with_backend(BackendId::Zypper)
            .with_locale("C")
            .with_privilege(p);
        c.args.extend(a.into_iter().map(OsString::from));
        c
    }
    pub fn search_plan(&self, q: &str) -> CommandPlan {
        self.plan(
            ["--xmlout", "search", "-s", "-t", "package", q],
            CommandPrivilege::User,
        )
    }
    pub fn list_plan(&self) -> CommandPlan {
        self.plan(
            ["--xmlout", "search", "-s", "-t", "package"],
            CommandPrivilege::User,
        )
    }
    pub fn installed_plan(&self) -> CommandPlan {
        self.plan(
            ["--xmlout", "search", "-s", "-i", "-t", "package"],
            CommandPrivilege::User,
        )
    }
    pub fn details_plan(&self, p: &PackageId) -> Result<CommandPlan, ZypperError> {
        if p.as_str().trim().is_empty() {
            Err(ZypperError::InvalidPackageId)
        } else {
            Ok(self.plan(["--xmlout", "info", p.as_str()], CommandPrivilege::User))
        }
    }
    pub fn updates_plan(&self) -> CommandPlan {
        self.plan(["--xmlout", "list-updates"], CommandPrivilege::User)
    }
    pub fn refresh_plan(&self) -> CommandPlan {
        self.plan(["refresh"], CommandPrivilege::Elevated)
    }
    pub fn system_upgrade_plan(&self) -> CommandPlan {
        self.plan(["update"], CommandPrivilege::Elevated)
    }
    pub fn transaction(&self, op: WriteOperation) -> Result<TransactionPlan, ZypperError> {
        let (v, ps) = match &op {
            WriteOperation::Install { packages } | WriteOperation::Upgrade { packages } => {
                ("install", packages)
            }
            WriteOperation::Remove { packages } => ("remove", packages),
            WriteOperation::SystemUpgrade => {
                return Ok(TransactionPlan {
                    backend: BackendId::Zypper,
                    kind: PackageKind::System,
                    scope: PackageScope::System,
                    operation: op,
                    command: self.system_upgrade_plan(),
                    packages: vec![],
                });
            }
            _ => return Err(ZypperError::UnsupportedOperation),
        };
        if ps.is_empty() {
            return Err(ZypperError::InvalidPackageId);
        }
        let mut c = self.plan([v], CommandPrivilege::Elevated);
        c.args
            .extend(ps.iter().map(|p| OsString::from(p.native_key.as_str())));
        Ok(TransactionPlan {
            backend: BackendId::Zypper,
            kind: PackageKind::System,
            scope: PackageScope::System,
            operation: op.clone(),
            command: c,
            packages: ps.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_consumer_decodes_entities_preserves_package_metadata_and_filters_other_solvables() {
        let packages = parse_search_xml(include_str!(
            "../../../../tests/package-managers/fixtures/zypper/search.xml"
        ))
        .unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "lib&foo");
        assert_eq!(packages[0].version, "1.0");
        assert_eq!(packages[0].architecture.as_deref(), Some("x86_64"));
        assert_eq!(packages[0].repository.as_deref(), Some("repo-oss"));
        assert!(packages[0].installed);
        println!("packages={packages:?}");

        let updates = parse_updates_xml(include_str!(
            "../../../../tests/package-managers/fixtures/zypper/updates.xml"
        ))
        .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].current.as_deref(), Some("1.0"));
        assert_eq!(updates[0].candidate, "1.1");
        println!("updates={updates:?}");
    }

    #[test]
    fn plans_keep_read_privilege_and_exact_write_argv() {
        let backend = ZypperBackend::from_paths("/fake/usr/bin/zypper");
        let search = backend.search_plan("lib;not-a-shell-command");
        assert_eq!(
            search.args,
            [
                "--xmlout",
                "search",
                "-s",
                "-t",
                "package",
                "lib;not-a-shell-command"
            ]
        );
        assert_eq!(search.privilege, CommandPrivilege::User);
        assert_eq!(
            backend
                .details_plan(&PackageId::new("bash").unwrap())
                .unwrap()
                .args,
            ["--xmlout", "info", "bash"]
        );
        assert_eq!(backend.updates_plan().args, ["--xmlout", "list-updates"]);
        assert_eq!(backend.refresh_plan().args, ["refresh"]);
        assert_eq!(backend.refresh_plan().privilege, CommandPrivilege::Elevated);
        assert_eq!(backend.system_upgrade_plan().args, ["update"]);

        for (operation, verb) in [
            (
                WriteOperation::Install {
                    packages: vec![package_identity("bash")],
                },
                "install",
            ),
            (
                WriteOperation::Remove {
                    packages: vec![package_identity("bash")],
                },
                "remove",
            ),
        ] {
            let plan = backend.transaction(operation).unwrap();
            assert_eq!(plan.command.args, [verb, "bash"]);
            assert_eq!(plan.command.privilege, CommandPrivilege::Elevated);
            println!("transaction={:?}", plan.command);
        }
    }

    #[test]
    fn malformed_truncated_and_missing_fields_fail() {
        assert!(matches!(
            parse_search_xml("<root><solvable"),
            Err(ZypperError::MalformedXml(_))
        ));
        assert!(matches!(
            parse_search_xml("<root><solvable type=\"package\"><name>x</name>"),
            Err(ZypperError::MalformedXml(_))
        ));
        assert!(matches!(
            parse_search_xml("<root><solvable type=\"package\"><name>x</name></solvable></root>"),
            Err(ZypperError::MissingField {
                field: "version",
                ..
            })
        ));
    }

    fn package_identity(name: &str) -> crate::PackageIdentity {
        crate::PackageIdentity::new(
            BackendId::Zypper,
            PackageKind::System,
            PackageScope::System,
            PackageId::new(name).unwrap(),
        )
    }
}
