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
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => {
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
                } else if let Some(x) = cur.as_mut() {
                    if matches!(t.as_str(), "repository" | "repo" | "alias") {
                        x.repo = attr(&e, "alias")
                            .or_else(|| attr(&e, "name"))
                            .or_else(|| attr(&e, "repo"));
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if let (Some(x), Some(f)) = (cur.as_mut(), field.as_deref()) {
                    let v = e
                        .decode()
                        .map_err(|e| ZypperError::MalformedXml(e.to_string()))?
                        .into_owned();
                    match f {
                        "name" => x.name = Some(v),
                        "version" | "evr" => x.version = Some(v),
                        "arch" | "architecture" => x.arch = Some(v),
                        "repository" | "repo" | "alias" => x.repo = Some(v),
                        "status" => x.installed = v.eq_ignore_ascii_case("installed"),
                        "current" => x.current = Some(v),
                        "candidate" => x.candidate = Some(v),
                        "type" | "kind" => x.kind = Some(v),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                if lname(e.name().as_ref()) == "solvable" {
                    if let Some(x) = cur.take() {
                        out.push(x)
                    }
                }
                field = None
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ZypperError::MalformedXml(e.to_string())),
            _ => {}
        }
    }
    Ok(out)
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
