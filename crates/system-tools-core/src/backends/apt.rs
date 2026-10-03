use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, PackageId, PackageKind,
    PackageScope, TransactionPlan, WriteOperation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AptPackage {
    pub name: String,
    pub version: String,
    pub architecture: Option<String>,
    pub installed: bool,
    pub held: bool,
    pub upgrade: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AptUpdate {
    pub name: String,
    pub current: Option<String>,
    pub candidate: String,
    pub architecture: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AptError {
    MalformedDeb822 { line: usize, reason: &'static str },
    MalformedDpkg { line: usize },
    MalformedSimulation { line: usize },
    InvalidPackageId,
    MissingExecutable(&'static str),
    UnsupportedOperation,
}

impl fmt::Display for AptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedDeb822 { line, reason } => {
                write!(f, "invalid Deb822 at line {line}: {reason}")
            }
            Self::MalformedDpkg { line } => write!(f, "invalid dpkg-query record at line {line}"),
            Self::MalformedSimulation { line } => {
                write!(f, "invalid apt simulation record at line {line}")
            }
            Self::InvalidPackageId => f.write_str("package id must not be empty"),
            Self::MissingExecutable(name) => write!(f, "required executable not found: {name}"),
            Self::UnsupportedOperation => f.write_str("APT does not support this operation"),
        }
    }
}
impl std::error::Error for AptError {}

fn fields_from_deb822(input: &str) -> Result<Vec<Vec<(String, String)>>, AptError> {
    let mut records = Vec::new();
    let mut current: Vec<(String, String)> = Vec::new();
    let mut last_key: Option<usize> = None;
    for (line_no, line) in input.lines().enumerate() {
        let number = line_no + 1;
        if line.trim().is_empty() {
            if !current.is_empty() {
                records.push(std::mem::take(&mut current));
            }
            last_key = None;
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            let Some(index) = last_key else {
                return Err(AptError::MalformedDeb822 {
                    line: number,
                    reason: "continuation without field",
                });
            };
            current[index].1.push('\n');
            current[index].1.push_str(line.trim_start());
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(AptError::MalformedDeb822 {
                line: number,
                reason: "missing colon",
            });
        };
        if key.trim().is_empty() {
            return Err(AptError::MalformedDeb822 {
                line: number,
                reason: "empty field",
            });
        }
        current.push((key.trim().to_ascii_lowercase(), value.trim().to_owned()));
        last_key = Some(current.len() - 1);
    }
    if !current.is_empty() {
        records.push(current);
    }
    Ok(records)
}

fn field<'a>(record: &'a [(String, String)], key: &str) -> Option<&'a str> {
    record
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

pub fn parse_deb822(input: &str) -> Result<Vec<AptPackage>, AptError> {
    fields_from_deb822(input)?
        .into_iter()
        .map(|record| {
            let name = field(&record, "package").ok_or(AptError::MalformedDeb822 {
                line: 0,
                reason: "missing Package",
            })?;
            if name.trim().is_empty() {
                return Err(AptError::MalformedDeb822 {
                    line: 0,
                    reason: "empty Package",
                });
            }
            Ok(AptPackage {
                name: name.to_owned(),
                version: field(&record, "version").unwrap_or_default().to_owned(),
                architecture: field(&record, "architecture").map(str::to_owned),
                installed: false,
                held: false,
                upgrade: None,
            })
        })
        .collect()
}

pub fn parse_dpkg_query(input: &str) -> Result<Vec<AptPackage>, AptError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_no, line)| {
            let parts: Vec<_> = line.split('\t').collect();
            if parts.len() < 4 || parts[0].trim().is_empty() {
                return Err(AptError::MalformedDpkg { line: line_no + 1 });
            }
            let status = parts[3];
            let status_parts: Vec<_> = status.split_whitespace().collect();
            let installed = status_parts.get(2) == Some(&"installed");
            Ok(AptPackage {
                name: parts[0].to_owned(),
                version: parts[1].to_owned(),
                architecture: Some(parts[2].to_owned()),
                installed,
                held: status.contains("hold"),
                upgrade: None,
            })
        })
        .collect()
}

pub fn parse_simulation(input: &str) -> Result<Vec<AptUpdate>, AptError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with("Inst "))
        .map(|(line_no, line)| {
            let text = line.trim_start().strip_prefix("Inst ").unwrap();
            let (name, rest) = text
                .split_once(' ')
                .ok_or(AptError::MalformedSimulation { line: line_no + 1 })?;
            let candidate = rest
                .split('(')
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .ok_or(AptError::MalformedSimulation { line: line_no + 1 })?;
            let current = rest
                .split_once('[')
                .and_then(|(_, s)| s.split(']').next())
                .map(str::to_owned);
            let architecture = name
                .rsplit_once(':')
                .and_then(|(_, arch)| (!arch.is_empty()).then(|| arch.to_owned()));
            Ok(AptUpdate {
                name: name.to_owned(),
                current,
                candidate: candidate.to_owned(),
                architecture,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct AptBackend {
    pub apt_cache: PathBuf,
    pub apt_get: PathBuf,
    pub dpkg_query: PathBuf,
}

impl AptBackend {
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, AptError> {
        let resolver = ExecutableResolver::from_path(path);
        let resolve = |name: &'static str| {
            resolver
                .resolve(OsStr::new(name))
                .ok_or(AptError::MissingExecutable(name))
        };
        Ok(Self {
            apt_cache: resolve("apt-cache")?,
            apt_get: resolve("apt-get")?,
            dpkg_query: resolve("dpkg-query")?,
        })
    }

    pub fn from_paths(
        apt_cache: impl Into<PathBuf>,
        apt_get: impl Into<PathBuf>,
        dpkg_query: impl Into<PathBuf>,
    ) -> Self {
        Self {
            apt_cache: apt_cache.into(),
            apt_get: apt_get.into(),
            dpkg_query: dpkg_query.into(),
        }
    }

    pub fn catalog_plan(&self) -> CommandPlan {
        read_plan(&self.apt_cache, ["dumpavail"])
    }
    pub fn installed_plan(&self) -> CommandPlan {
        read_plan(
            &self.dpkg_query,
            [
                "-W",
                "--showformat=${Package}\\t${Version}\\t${Architecture}\\t${Status}\\n",
            ],
        )
    }
    pub fn details_plan(&self, package: &PackageId) -> Result<CommandPlan, AptError> {
        if package.as_str().trim().is_empty() {
            return Err(AptError::InvalidPackageId);
        }
        Ok(read_plan(
            &self.apt_cache,
            ["show", "--no-all-versions", package.as_str()],
        ))
    }
    pub fn updates_plan(&self) -> CommandPlan {
        read_plan(&self.apt_get, ["--just-print", "--simulate", "upgrade"])
    }
    pub fn update_catalog_plan(&self) -> CommandPlan {
        elevated_plan(&self.apt_get, ["update"])
    }
    pub fn system_upgrade_plan(&self) -> CommandPlan {
        elevated_plan(&self.apt_get, ["full-upgrade"])
    }

    pub fn transaction(&self, operation: WriteOperation) -> Result<TransactionPlan, AptError> {
        let (verb, packages): (&str, &[crate::PackageIdentity]) = match &operation {
            WriteOperation::Install { packages } => ("install", packages),
            WriteOperation::Remove { packages } => ("remove", packages),
            WriteOperation::Upgrade { packages } => ("install", packages),
            WriteOperation::Downgrade { .. } => return Err(AptError::UnsupportedOperation),
            WriteOperation::SystemUpgrade => {
                return Ok(TransactionPlan {
                    backend: BackendId::Apt,
                    kind: PackageKind::System,
                    scope: PackageScope::System,
                    operation,
                    command: self.system_upgrade_plan(),
                    packages: Vec::new(),
                });
            }
        };
        if packages.is_empty()
            || packages
                .iter()
                .any(|p| p.native_key.as_str().trim().is_empty())
        {
            return Err(AptError::InvalidPackageId);
        }
        let package_identities = packages.to_vec();
        let mut command = elevated_plan(&self.apt_get, [verb]);
        command.args.extend(
            packages
                .iter()
                .map(|p| OsString::from(p.native_key.as_str())),
        );
        Ok(TransactionPlan {
            backend: BackendId::Apt,
            kind: PackageKind::System,
            scope: PackageScope::System,
            operation,
            command,
            packages: package_identities,
        })
    }
}

fn read_plan<const N: usize>(program: &Path, args: [&str; N]) -> CommandPlan {
    let mut plan = CommandPlan::new(program.to_path_buf())
        .with_backend(BackendId::Apt)
        .with_locale("C")
        .with_privilege(CommandPrivilege::User);
    plan.args.extend(args.into_iter().map(OsString::from));
    plan
}
fn elevated_plan<const N: usize>(program: &Path, args: [&str; N]) -> CommandPlan {
    read_plan(program, args).with_privilege(CommandPrivilege::Elevated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageId, PackageIdentity, WriteOperation};

    fn identity(name: &str) -> PackageIdentity {
        PackageIdentity::new(
            BackendId::Apt,
            PackageKind::System,
            PackageScope::System,
            PackageId::new(name).unwrap(),
        )
    }

    #[test]
    fn parses_deb822_and_continuations() {
        let records = parse_deb822("Package: libc6:amd64\nVersion: 2.36\nArchitecture: amd64\nDescription: first\n second line\n\n").unwrap();
        assert_eq!(records[0].name, "libc6:amd64");
        assert_eq!(records[0].architecture.as_deref(), Some("amd64"));
    }

    #[test]
    fn fixture_consumer_prints_parsed_records_and_exact_plans() {
        let catalog = parse_deb822(include_str!(
            "../../../../tests/package-managers/fixtures/apt/catalog.deb822"
        ))
        .unwrap();
        let installed = parse_dpkg_query(include_str!(
            "../../../../tests/package-managers/fixtures/apt/installed.tsv"
        ))
        .unwrap();
        let updates = parse_simulation(include_str!(
            "../../../../tests/package-managers/fixtures/apt/updates.txt"
        ))
        .unwrap();
        let backend = AptBackend::from_paths(
            "/fake/usr/bin/apt-cache",
            "/fake/usr/bin/apt-get",
            "/fake/usr/bin/dpkg-query",
        );
        let transaction = backend
            .transaction(WriteOperation::Install {
                packages: vec![identity("bash:amd64")],
            })
            .unwrap();
        println!("catalog={catalog:?}");
        println!("installed={installed:?}");
        println!("updates={updates:?}");
        println!("install_plan={transaction:?}");
        assert_eq!(catalog.len(), 2);
        assert_eq!(installed.len(), 3);
        assert_eq!(updates.len(), 2);
        assert_eq!(
            transaction.command.program,
            PathBuf::from("/fake/usr/bin/apt-get")
        );
        assert_eq!(transaction.command.args, ["install", "bash:amd64"]);
    }
    #[test]
    fn parses_dpkg_status_and_hold() {
        let records = parse_dpkg_query(
            "bash\t5.2\tamd64\tinstall ok installed\nlinux\t6\tamd64\thold ok installed\n",
        )
        .unwrap();
        assert!(records[0].installed);
        assert!(!records[0].held);
        assert!(records[1].installed);
        assert!(records[1].held);
    }
    #[test]
    fn parses_simulated_updates_without_trusting_display_text() {
        let records = parse_simulation(
            "Inst libc6:amd64 [2.36] (2.37 Debian:stable)\nConf libc6:amd64 (2.37)\n",
        )
        .unwrap();
        assert_eq!(records[0].name, "libc6:amd64");
        assert_eq!(records[0].current.as_deref(), Some("2.36"));
        assert_eq!(records[0].candidate, "2.37");
    }
    #[test]
    fn plans_use_absolute_paths_and_no_automation_flags() {
        let backend = AptBackend::from_paths(
            "/usr/bin/apt-cache",
            "/usr/bin/apt-get",
            "/usr/bin/dpkg-query",
        );
        let id = PackageId::new("bash:amd64").unwrap();
        let plan = backend
            .transaction(WriteOperation::Install {
                packages: vec![identity(id.as_str())],
            })
            .unwrap()
            .command;
        assert_eq!(plan.program, PathBuf::from("/usr/bin/apt-get"));
        assert_eq!(plan.privilege, CommandPrivilege::Elevated);
        assert!(
            !plan
                .args
                .iter()
                .any(|arg| arg == "-y" || arg == "--yes" || arg == "--non-interactive")
        );
    }
    #[test]
    fn downgrade_is_rejected_before_spawn() {
        let backend = AptBackend::from_paths(
            "/usr/bin/apt-cache",
            "/usr/bin/apt-get",
            "/usr/bin/dpkg-query",
        );
        let err = backend
            .transaction(WriteOperation::Downgrade {
                packages: vec![identity("x")],
            })
            .unwrap_err();
        assert_eq!(err, AptError::UnsupportedOperation);
    }

    #[test]
    fn malformed_inputs_and_untrusted_package_text_are_bounded() {
        assert!(matches!(
            parse_deb822(" continuation\n"),
            Err(AptError::MalformedDeb822 { .. })
        ));
        assert!(matches!(
            parse_dpkg_query("broken\n"),
            Err(AptError::MalformedDpkg { .. })
        ));
        assert!(matches!(
            parse_simulation("Inst package-without-version\n"),
            Err(AptError::MalformedSimulation { .. })
        ));
        let backend = AptBackend::from_paths("/apt-cache", "/apt-get", "/dpkg-query");
        let id = PackageId::new("pkg; touch /tmp/not-run").unwrap();
        let plan = backend
            .transaction(WriteOperation::Install {
                packages: vec![identity(id.as_str())],
            })
            .unwrap();
        assert_eq!(plan.command.args, ["install", "pkg; touch /tmp/not-run"]);
    }
}
