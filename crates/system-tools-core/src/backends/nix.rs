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
    ExperimentalFeaturesDisabled,
    UnsupportedOperation,
}
impl fmt::Display for NixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::MalformedJson(e)=>write!(f,"invalid nix JSON: {e}"), Self::MissingField{record,field}=>write!(f,"nix record {record} is missing {field}"), Self::InvalidSelector=>f.write_str("nix profile selector must not be empty"), Self::MissingExecutable(n)=>write!(f,"required executable not found: {n}"), Self::ExperimentalFeaturesDisabled=>f.write_str("nix-command and flakes experimental features are required; enable them in the invoking environment, nix.conf was not changed"), Self::UnsupportedOperation=>f.write_str("nix operation is unsupported") }
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
    pub fn transaction(&self, op: WriteOperation) -> Result<TransactionPlan, NixError> {
        let (verb, ps) = match &op {
            WriteOperation::Install { packages } => ("install", packages),
            WriteOperation::Remove { packages } => ("remove", packages),
            WriteOperation::Upgrade { packages } => ("upgrade", packages),
            _ => return Err(NixError::UnsupportedOperation),
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
        let mut c = self.plan(["profile", "install"]);
        if verb == "remove" {
            c.args = vec!["profile".into(), "remove".into()];
        } else if verb == "upgrade" {
            c.args = vec!["profile".into(), "upgrade".into()];
        }
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
    fn plans_are_user_only_and_selector_based() {
        let b = NixBackend::from_paths("/fake/nix");
        let i = PackageIdentity::new(
            BackendId::Nix,
            PackageKind::Nix,
            PackageScope::Profile,
            PackageId::new("1").unwrap(),
        );
        let p = b
            .transaction(WriteOperation::Remove { packages: vec![i] })
            .unwrap();
        assert_eq!(p.command.args, ["profile", "remove", "1"]);
        assert_eq!(p.command.privilege, CommandPrivilege::User);
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
}
