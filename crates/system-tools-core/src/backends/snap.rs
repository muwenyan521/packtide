use crate::{
    BackendId, CommandPlan, CommandPrivilege, ExecutableResolver, NativePackageKey, PackageId,
    PackageIdentity, PackageKind, PackageScope, TransactionPlan, WriteOperation,
};
use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SnapConfinement {
    Strict,
    Classic,
    Devmode,
}

impl SnapConfinement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Classic => "classic",
            Self::Devmode => "devmode",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "strict" => Some(Self::Strict),
            "classic" => Some(Self::Classic),
            "devmode" | "dev-mode" => Some(Self::Devmode),
            _ => None,
        }
    }
}

impl fmt::Display for SnapConfinement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SnapPackageId {
    pub name: String,
    pub channel: String,
    pub confinement: SnapConfinement,
    pub scope: PackageScope,
}

impl SnapPackageId {
    pub const DEFAULT_CHANNEL: &'static str = "latest/stable";

    pub fn new(
        name: impl Into<String>,
        channel: impl Into<String>,
        confinement: SnapConfinement,
        scope: PackageScope,
    ) -> Result<Self, SnapError> {
        let id = Self {
            name: name.into(),
            channel: channel.into(),
            confinement,
            scope,
        };
        id.validate()?;
        Ok(id)
    }

    pub fn encode(&self) -> String {
        format!("{}@{}#{}", self.name, self.channel, self.confinement)
    }

    pub fn to_package_id(&self) -> Result<PackageId, SnapError> {
        PackageId::new(self.encode()).map_err(|_| SnapError::InvalidPackageId)
    }

    fn validate(&self) -> Result<(), SnapError> {
        validate_snap_name(&self.name)?;
        validate_channel(&self.channel)?;
        if self.scope != PackageScope::System {
            return Err(SnapError::UnsupportedScope(self.scope));
        }
        Ok(())
    }

    pub fn parse(value: &str) -> Result<Self, SnapError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(SnapError::InvalidPackageId);
        }
        if value.starts_with('/')
            || value.starts_with("./")
            || value.starts_with("../")
            || value == "--dangerous"
            || value.ends_with(".snap")
        {
            return Err(SnapError::UnsupportedInstallSource);
        }
        let (name, channel, confinement) = if let Some((name, rest)) = value.split_once('@') {
            if name.ends_with(".snap") {
                return Err(SnapError::UnsupportedInstallSource);
            }
            let (channel, mode) = rest.rsplit_once('#').ok_or(SnapError::InvalidPackageId)?;
            let mode = SnapConfinement::parse(mode).ok_or(SnapError::InvalidConfinement)?;
            (name, channel, mode)
        } else {
            (value, Self::DEFAULT_CHANNEL, SnapConfinement::Strict)
        };
        Self::new(name, channel, confinement, PackageScope::System)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapPackage {
    pub name: String,
    pub version: Option<String>,
    pub revision: Option<String>,
    pub publisher: Option<String>,
    pub summary: Option<String>,
    pub channel: String,
    pub confinement: SnapConfinement,
    pub scope: PackageScope,
    pub installed: bool,
}

impl SnapPackage {
    pub fn package_id(&self) -> Result<SnapPackageId, SnapError> {
        SnapPackageId::new(
            self.name.clone(),
            self.channel.clone(),
            self.confinement,
            self.scope,
        )
    }

    pub fn identity(&self) -> Result<PackageIdentity, SnapError> {
        let id = self.package_id()?;
        let mut identity = PackageIdentity::new(
            BackendId::Snap,
            PackageKind::Snap,
            self.scope,
            NativePackageKey::new(id.encode()).map_err(|_| SnapError::InvalidPackageId)?,
        );
        if let Some(publisher) = self.publisher.as_deref().filter(|v| !v.trim().is_empty()) {
            identity = identity.with_origin(publisher);
        }
        if let Some(summary) = self.summary.as_deref().filter(|v| !v.trim().is_empty()) {
            identity = identity.with_display_name(summary);
        }
        Ok(identity)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapUpdate {
    pub name: String,
    pub current: Option<String>,
    pub candidate: String,
    pub revision: Option<String>,
    pub publisher: Option<String>,
    pub channel: String,
    pub confinement: SnapConfinement,
}

impl SnapUpdate {
    pub fn identity(&self) -> Result<PackageIdentity, SnapError> {
        SnapPackage {
            name: self.name.clone(),
            version: Some(self.candidate.clone()),
            revision: self.revision.clone(),
            publisher: self.publisher.clone(),
            summary: None,
            channel: self.channel.clone(),
            confinement: self.confinement,
            scope: PackageScope::System,
            installed: false,
        }
        .identity()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapInstallConfirmation {
    pub publisher: String,
    pub channel: String,
    pub confinement: SnapConfinement,
    pub confirmed: bool,
}

impl SnapInstallConfirmation {
    pub fn new(
        publisher: impl Into<String>,
        channel: impl Into<String>,
        confinement: SnapConfinement,
    ) -> Result<Self, SnapError> {
        let publisher = publisher.into();
        if publisher.trim().is_empty() {
            return Err(SnapError::MissingConfirmationField("publisher"));
        }
        let channel = channel.into();
        validate_channel(&channel)?;
        Ok(Self {
            publisher,
            channel,
            confinement,
            confirmed: false,
        })
    }

    pub fn confirmed(mut self) -> Self {
        self.confirmed = true;
        self
    }

    pub fn confirm(&mut self) {
        self.confirmed = true;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapError {
    MalformedRecord { line: usize, reason: &'static str },
    MissingField { line: usize, field: &'static str },
    InvalidPackageId,
    InvalidSnapName,
    InvalidChannel,
    InvalidConfinement,
    MissingConfirmationField(&'static str),
    ConfirmationRequired { confinement: SnapConfinement },
    ConfirmationMismatch { field: &'static str },
    UnsupportedInstallSource,
    UnsupportedScope(PackageScope),
    MissingExecutable(&'static str),
    UnsupportedOperation,
}

impl fmt::Display for SnapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedRecord { line, reason } => {
                write!(f, "invalid snap record at line {line}: {reason}")
            }
            Self::MissingField { line, field } => {
                write!(f, "snap record at line {line} is missing {field}")
            }
            Self::InvalidPackageId => f.write_str("snap package id is invalid"),
            Self::InvalidSnapName => f.write_str("snap name is invalid"),
            Self::InvalidChannel => f.write_str("snap channel is invalid"),
            Self::InvalidConfinement => f.write_str("snap confinement is invalid"),
            Self::MissingConfirmationField(field) => {
                write!(f, "snap install confirmation is missing {field}")
            }
            Self::ConfirmationRequired { confinement } => {
                write!(
                    f,
                    "snap {confinement} install requires explicit confirmation"
                )
            }
            Self::ConfirmationMismatch { field } => {
                write!(f, "snap install confirmation does not match {field}")
            }
            Self::UnsupportedInstallSource => {
                f.write_str("dangerous and local snap installs are unsupported")
            }
            Self::UnsupportedScope(scope) => {
                write!(f, "snap backend does not support {scope:?} scope")
            }
            Self::MissingExecutable(name) => write!(f, "required executable not found: {name}"),
            Self::UnsupportedOperation => f.write_str("snap does not support this operation"),
        }
    }
}

impl std::error::Error for SnapError {}

fn validate_snap_name(name: &str) -> Result<(), SnapError> {
    if name.is_empty()
        || name.len() > 40
        || name.starts_with('-')
        || name.ends_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(SnapError::InvalidSnapName);
    }
    Ok(())
}

fn validate_channel(channel: &str) -> Result<(), SnapError> {
    if channel.is_empty()
        || channel.starts_with('-')
        || channel.ends_with('/')
        || channel.contains("..")
        || !channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".+/-".contains(&byte))
        || channel
            .split('/')
            .any(|part| part.is_empty() || part == ".")
    {
        return Err(SnapError::InvalidChannel);
    }
    Ok(())
}

fn mode_from_notes(notes: &str) -> Option<SnapConfinement> {
    let notes = notes.to_ascii_lowercase();
    if notes
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part == "classic")
    {
        Some(SnapConfinement::Classic)
    } else if notes
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part == "devmode" || part == "dev")
    {
        Some(SnapConfinement::Devmode)
    } else if notes.split_whitespace().any(|part| part == "-") {
        Some(SnapConfinement::Strict)
    } else {
        None
    }
}

fn clean_field(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value != "-").then(|| value.to_owned())
}

fn revision_token(value: &str) -> Option<String> {
    let value = value.trim_matches(['(', ')']);
    (!value.is_empty() && value.chars().all(|character| character.is_ascii_digit()))
        .then(|| value.to_owned())
}

fn parse_table_header(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|field| field.to_ascii_lowercase())
        .collect()
}

fn is_header(fields: &[String], expected: &[&str]) -> bool {
    !fields.is_empty()
        && expected
            .iter()
            .all(|field| fields.iter().any(|v| v == field))
}

fn table_line_error(line: usize, reason: &'static str) -> SnapError {
    SnapError::MalformedRecord { line, reason }
}

pub fn parse_find(input: &str) -> Result<Vec<SnapPackage>, SnapError> {
    let mut saw_header = false;
    let mut rows = Vec::new();
    for (line_no, line) in input.lines().enumerate() {
        let number = line_no + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("WARNING:") {
            continue;
        }
        let fields = parse_table_header(trimmed);
        if !saw_header && is_header(&fields, &["name", "version", "publisher", "summary"]) {
            saw_header = true;
            continue;
        }
        if !saw_header {
            continue;
        }
        let mut values = trimmed.split_whitespace();
        let name = values
            .next()
            .ok_or_else(|| table_line_error(number, "missing name"))?;
        validate_snap_name(name)?;
        let version = clean_field(
            values
                .next()
                .ok_or_else(|| table_line_error(number, "missing version"))?,
        );
        let publisher = clean_field(
            values
                .next()
                .ok_or_else(|| table_line_error(number, "missing publisher"))?,
        );
        let notes = values
            .next()
            .ok_or_else(|| table_line_error(number, "missing mode"))?;
        let confinement =
            mode_from_notes(notes).ok_or_else(|| table_line_error(number, "missing mode"))?;
        let summary = clean_field(&values.collect::<Vec<_>>().join(" "));
        rows.push(SnapPackage {
            name: name.to_owned(),
            version,
            revision: None,
            publisher,
            summary,
            channel: SnapPackageId::DEFAULT_CHANNEL.to_owned(),
            confinement,
            scope: PackageScope::System,
            installed: false,
        });
    }
    if !saw_header {
        return Err(table_line_error(1, "missing find header"));
    }
    Ok(rows)
}

pub fn parse_info(input: &str) -> Result<Vec<SnapPackage>, SnapError> {
    let mut name = None;
    let mut publisher = None;
    let mut summary = None;
    let mut top_version = None;
    let mut top_revision = None;
    let mut top_confinement = None;
    let mut channels = Vec::new();
    let mut in_channels = false;
    for (line_no, raw) in input.lines().enumerate() {
        let number = line_no + 1;
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("channels:") {
            in_channels = true;
            continue;
        }
        if in_channels
            && raw.chars().next().is_some_and(char::is_whitespace)
            && let Some((channel, rest)) = trimmed.split_once(':')
        {
            if validate_channel(channel).is_err() {
                continue;
            }
            let tokens: Vec<_> = rest.split_whitespace().collect();
            let candidate = tokens.first().and_then(|v| clean_field(v));
            let revision = tokens.iter().find_map(|token| revision_token(token));
            let notes = tokens.last().copied().unwrap_or_default();
            let confinement = mode_from_notes(notes).ok_or(SnapError::MissingField {
                line: number,
                field: "mode",
            })?;
            channels.push((channel.to_owned(), candidate, revision, confinement));
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "name" => name = clean_field(value),
            "publisher" => publisher = clean_field(value),
            "summary" => summary = clean_field(value),
            "version" => top_version = clean_field(value),
            "revision" => top_revision = clean_field(value),
            "confinement" | "mode" => top_confinement = SnapConfinement::parse(value),
            _ => {}
        }
        if key != "channels"
            && raw
                .chars()
                .next()
                .is_some_and(|character| !character.is_whitespace())
        {
            in_channels = false;
        }
        if key == "confinement" && top_confinement.is_none() {
            return Err(SnapError::MissingField {
                line: number,
                field: "mode",
            });
        }
    }
    let name = name.ok_or(SnapError::MissingField {
        line: 1,
        field: "name",
    })?;
    validate_snap_name(&name)?;
    if channels.is_empty() {
        let confinement = top_confinement.ok_or(SnapError::MissingField {
            line: 1,
            field: "mode",
        })?;
        return Ok(vec![SnapPackage {
            name,
            version: top_version,
            revision: top_revision,
            publisher,
            summary,
            channel: SnapPackageId::DEFAULT_CHANNEL.to_owned(),
            confinement,
            scope: PackageScope::System,
            installed: false,
        }]);
    }
    Ok(channels
        .into_iter()
        .map(|(channel, version, revision, confinement)| SnapPackage {
            name: name.clone(),
            version,
            revision,
            publisher: publisher.clone(),
            summary: summary.clone(),
            channel,
            confinement,
            scope: PackageScope::System,
            installed: false,
        })
        .collect())
}

pub fn parse_list(input: &str) -> Result<Vec<SnapPackage>, SnapError> {
    let mut saw_header = false;
    let mut rows = Vec::new();
    for (line_no, line) in input.lines().enumerate() {
        let number = line_no + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("WARNING:") {
            continue;
        }
        let fields = parse_table_header(trimmed);
        if !saw_header && is_header(&fields, &["name", "version", "rev", "tracking", "notes"]) {
            saw_header = true;
            continue;
        }
        if !saw_header {
            continue;
        }
        let values: Vec<_> = trimmed.split_whitespace().collect();
        if values.len() < 6 {
            return Err(table_line_error(
                number,
                "expected name, version, revision, tracking, publisher and mode",
            ));
        }
        validate_snap_name(values[0])?;
        let confinement = mode_from_notes(values[5..].join(" ").as_str())
            .ok_or_else(|| table_line_error(number, "missing mode"))?;
        rows.push(SnapPackage {
            name: values[0].to_owned(),
            version: clean_field(values[1]),
            revision: clean_field(values[2]),
            publisher: clean_field(values[4]),
            summary: None,
            channel: clean_field(values[3])
                .unwrap_or_else(|| SnapPackageId::DEFAULT_CHANNEL.to_owned()),
            confinement,
            scope: PackageScope::System,
            installed: true,
        });
    }
    if !saw_header {
        return Err(table_line_error(1, "missing list header"));
    }
    Ok(rows)
}

pub fn parse_refresh_list(input: &str) -> Result<Vec<SnapUpdate>, SnapError> {
    let mut saw_header = false;
    let mut rows = Vec::new();
    for (line_no, line) in input.lines().enumerate() {
        let number = line_no + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("WARNING:") {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("all snaps up to date.") {
            saw_header = true;
            continue;
        }
        let fields = parse_table_header(trimmed);
        if !saw_header && is_header(&fields, &["name", "version", "rev", "notes"]) {
            saw_header = true;
            continue;
        }
        if !saw_header {
            continue;
        }
        let values: Vec<_> = trimmed.split_whitespace().collect();
        if values.len() < 5 {
            return Err(table_line_error(
                number,
                "expected name, version, revision, publisher and mode",
            ));
        }
        validate_snap_name(values[0])?;
        let confinement = mode_from_notes(values[4..].join(" ").as_str())
            .ok_or_else(|| table_line_error(number, "missing mode"))?;
        rows.push(SnapUpdate {
            name: values[0].to_owned(),
            current: None,
            candidate: values[1].to_owned(),
            revision: clean_field(values[2]),
            publisher: clean_field(values[3]),
            channel: SnapPackageId::DEFAULT_CHANNEL.to_owned(),
            confinement,
        });
    }
    if !saw_header {
        return Err(table_line_error(1, "missing refresh header"));
    }
    Ok(rows)
}

pub fn parse_search(input: &str) -> Result<Vec<SnapPackage>, SnapError> {
    parse_find(input)
}

pub fn parse_installed(input: &str) -> Result<Vec<SnapPackage>, SnapError> {
    parse_list(input)
}

pub fn parse_updates(input: &str) -> Result<Vec<SnapUpdate>, SnapError> {
    parse_refresh_list(input)
}

#[derive(Clone, Debug)]
pub struct SnapBackend {
    pub program: PathBuf,
}

impl SnapBackend {
    pub fn from_path(path: Option<&OsStr>) -> Result<Self, SnapError> {
        let program = ExecutableResolver::from_path(path)
            .resolve(OsStr::new("snap"))
            .ok_or(SnapError::MissingExecutable("snap"))?;
        Ok(Self { program })
    }

    pub fn from_paths(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

    fn plan<const N: usize>(&self, args: [&str; N], privilege: CommandPrivilege) -> CommandPlan {
        let mut plan = CommandPlan::new(self.program.clone())
            .with_backend(BackendId::Snap)
            .with_locale("C")
            .with_privilege(privilege);
        plan.args.extend(args.into_iter().map(OsString::from));
        plan
    }

    pub fn search_plan(&self, query: &str) -> CommandPlan {
        self.plan(["find", query], CommandPrivilege::User)
    }

    pub fn find_plan(&self, query: &str) -> CommandPlan {
        self.search_plan(query)
    }

    pub fn list_plan(&self) -> CommandPlan {
        self.plan(["list"], CommandPrivilege::User)
    }

    pub fn installed_plan(&self) -> CommandPlan {
        self.list_plan()
    }

    pub fn details_plan(&self, package: &PackageId) -> Result<CommandPlan, SnapError> {
        let id = SnapPackageId::parse(package.as_str())?;
        Ok(self.plan(["info", &id.name], CommandPrivilege::User))
    }

    pub fn updates_plan(&self) -> CommandPlan {
        self.plan(["refresh", "--list"], CommandPrivilege::User)
    }

    pub fn refresh_plan(&self) -> CommandPlan {
        self.plan(["refresh"], CommandPrivilege::Elevated)
    }

    pub fn update_catalog_plan(&self) -> CommandPlan {
        self.refresh_plan()
    }

    pub fn system_upgrade_plan(&self) -> CommandPlan {
        self.refresh_plan()
    }

    pub fn identity(&self, package: &SnapPackage) -> Result<PackageIdentity, SnapError> {
        package.identity()
    }

    pub fn install_plan(
        &self,
        package: &PackageIdentity,
        confirmation: Option<&SnapInstallConfirmation>,
    ) -> Result<CommandPlan, SnapError> {
        validate_identity(package)?;
        let id = SnapPackageId::parse(package.native_key.as_str())?;
        let mut args = vec![OsString::from("install"), OsString::from(id.name.clone())];
        if id.channel != SnapPackageId::DEFAULT_CHANNEL {
            args.push(OsString::from(format!("--channel={}", id.channel)));
        }
        validate_confirmation(package, &id, confirmation)?;
        match id.confinement {
            SnapConfinement::Strict => {}
            SnapConfinement::Classic => args.push(OsString::from("--classic")),
            SnapConfinement::Devmode => args.push(OsString::from("--devmode")),
        }
        let mut plan = self.plan::<0>([], CommandPrivilege::Elevated);
        plan.args = args;
        Ok(plan)
    }

    pub fn transaction(&self, operation: WriteOperation) -> Result<TransactionPlan, SnapError> {
        self.transaction_with_confirmation(operation, None)
    }

    pub fn transaction_with_confirmation(
        &self,
        operation: WriteOperation,
        confirmation: Option<&SnapInstallConfirmation>,
    ) -> Result<TransactionPlan, SnapError> {
        let operation_for_plan = operation.clone();
        let packages = operation.packages();
        let command = match &operation {
            WriteOperation::Install { packages } => {
                if packages.is_empty() {
                    return Err(SnapError::InvalidPackageId);
                }
                let first = self.install_plan(&packages[0], confirmation)?;
                if packages.len() > 1 {
                    let first_id = SnapPackageId::parse(packages[0].native_key.as_str())?;
                    for package in &packages[1..] {
                        validate_identity(package)?;
                        let id = SnapPackageId::parse(package.native_key.as_str())?;
                        if id.channel != first_id.channel || id.confinement != first_id.confinement
                        {
                            return Err(SnapError::UnsupportedOperation);
                        }
                        validate_confirmation(package, &id, confirmation)?;
                    }
                    let mut command = self.plan::<0>([], CommandPrivilege::Elevated);
                    command.args.push(OsString::from("install"));
                    command.args.extend(packages.iter().map(|package| {
                        OsString::from(
                            SnapPackageId::parse(package.native_key.as_str())
                                .expect("validated snap package id")
                                .name,
                        )
                    }));
                    if first_id.channel != SnapPackageId::DEFAULT_CHANNEL {
                        command
                            .args
                            .push(OsString::from(format!("--channel={}", first_id.channel)));
                    }
                    match first_id.confinement {
                        SnapConfinement::Strict => {}
                        SnapConfinement::Classic => command.args.push(OsString::from("--classic")),
                        SnapConfinement::Devmode => command.args.push(OsString::from("--devmode")),
                    }
                    command
                } else {
                    first
                }
            }
            WriteOperation::Remove { packages } => {
                let names = package_names(packages)?;
                let mut command = self.plan::<0>([], CommandPrivilege::Elevated);
                command.args.push(OsString::from("remove"));
                command.args.extend(names.into_iter().map(OsString::from));
                command
            }
            WriteOperation::Upgrade { packages } => {
                let names = package_names(packages)?;
                let mut command = self.plan::<0>([], CommandPrivilege::Elevated);
                command.args.push(OsString::from("refresh"));
                command.args.extend(names.into_iter().map(OsString::from));
                command
            }
            WriteOperation::SystemUpgrade => self.system_upgrade_plan(),
            WriteOperation::Downgrade { .. } => return Err(SnapError::UnsupportedOperation),
        };
        let scope = packages.first().map_or(PackageScope::System, |p| p.scope);
        if scope != PackageScope::System {
            return Err(SnapError::UnsupportedScope(scope));
        }
        Ok(TransactionPlan {
            backend: BackendId::Snap,
            kind: PackageKind::Snap,
            scope: PackageScope::System,
            operation: operation_for_plan,
            command,
            packages: packages.to_vec(),
        })
    }
}

fn validate_identity(package: &PackageIdentity) -> Result<(), SnapError> {
    if package.backend != BackendId::Snap || package.kind != PackageKind::Snap {
        return Err(SnapError::InvalidPackageId);
    }
    if package.scope != PackageScope::System {
        return Err(SnapError::UnsupportedScope(package.scope));
    }
    SnapPackageId::parse(package.native_key.as_str()).map(|_| ())
}

fn package_names(packages: &[PackageIdentity]) -> Result<Vec<String>, SnapError> {
    if packages.is_empty() {
        return Err(SnapError::InvalidPackageId);
    }
    packages
        .iter()
        .map(|package| {
            validate_identity(package)?;
            Ok(SnapPackageId::parse(package.native_key.as_str())?.name)
        })
        .collect()
}

fn validate_confirmation(
    package: &PackageIdentity,
    id: &SnapPackageId,
    confirmation: Option<&SnapInstallConfirmation>,
) -> Result<(), SnapError> {
    if id.confinement == SnapConfinement::Strict {
        return Ok(());
    }
    let confirmation = confirmation.ok_or(SnapError::ConfirmationRequired {
        confinement: id.confinement,
    })?;
    if !confirmation.confirmed {
        return Err(SnapError::ConfirmationRequired {
            confinement: id.confinement,
        });
    }
    if confirmation.publisher.trim().is_empty() {
        return Err(SnapError::MissingConfirmationField("publisher"));
    }
    if confirmation.publisher != package.origin.as_deref().unwrap_or_default() {
        return Err(SnapError::ConfirmationMismatch { field: "publisher" });
    }
    if confirmation.channel != id.channel {
        return Err(SnapError::ConfirmationMismatch { field: "channel" });
    }
    if confirmation.confinement != id.confinement {
        return Err(SnapError::ConfirmationMismatch {
            field: "confinement",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WriteOperation;

    fn fixture_identity(
        name: &str,
        channel: &str,
        mode: SnapConfinement,
        publisher: &str,
    ) -> PackageIdentity {
        let package = SnapPackage {
            name: name.to_owned(),
            version: Some("1.0".to_owned()),
            revision: Some("1".to_owned()),
            publisher: Some(publisher.to_owned()),
            summary: Some("fixture".to_owned()),
            channel: channel.to_owned(),
            confinement: mode,
            scope: PackageScope::System,
            installed: false,
        };
        package.identity().unwrap()
    }

    #[test]
    fn find_info_list_and_refresh_fixtures_preserve_snap_identity() {
        let found = parse_find(include_str!(
            "../../../../tests/package-managers/fixtures/snap/find.txt"
        ))
        .unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].name, "hello-world");
        assert_eq!(found[1].confinement, SnapConfinement::Classic);
        assert_eq!(found[2].confinement, SnapConfinement::Devmode);

        let info = parse_info(include_str!(
            "../../../../tests/package-managers/fixtures/snap/info.txt"
        ))
        .unwrap();
        assert_eq!(info.len(), 2);
        assert_eq!(info[1].channel, "latest/candidate");
        assert_eq!(info[0].revision.as_deref(), Some("123"));
        assert_eq!(info[0].identity().unwrap().scope, PackageScope::System);
        assert!(
            info[0]
                .identity()
                .unwrap()
                .native_key
                .as_str()
                .contains("#strict")
        );

        let installed = parse_list(include_str!(
            "../../../../tests/package-managers/fixtures/snap/list.txt"
        ))
        .unwrap();
        assert_eq!(installed[0].channel, "latest/stable");
        assert!(installed[0].installed);

        let updates = parse_refresh_list(include_str!(
            "../../../../tests/package-managers/fixtures/snap/refresh-list.txt"
        ))
        .unwrap();
        assert_eq!(updates[0].candidate, "1.3");
        assert_eq!(updates[0].confinement, SnapConfinement::Classic);
    }

    #[test]
    fn fixture_consumer_prints_strict_classic_devmode_and_rejected_outcomes() {
        let backend = SnapBackend::from_paths("/fake/usr/bin/snap");
        let strict = fixture_identity(
            "hello-world",
            SnapPackageId::DEFAULT_CHANNEL,
            SnapConfinement::Strict,
            "canonical",
        );
        let classic = fixture_identity(
            "classic-app",
            "latest/stable",
            SnapConfinement::Classic,
            "acme",
        );
        let devmode = fixture_identity("dev-app", "2/stable", SnapConfinement::Devmode, "acme");
        let strict_plan = backend
            .transaction(WriteOperation::Install {
                packages: vec![strict.clone()],
            })
            .unwrap();
        let confirmation =
            SnapInstallConfirmation::new("acme", "latest/stable", SnapConfinement::Classic)
                .unwrap()
                .confirmed();
        let classic_plan = backend
            .transaction_with_confirmation(
                WriteOperation::Install {
                    packages: vec![classic.clone()],
                },
                Some(&confirmation),
            )
            .unwrap();
        let dev_confirmation =
            SnapInstallConfirmation::new("acme", "2/stable", SnapConfinement::Devmode)
                .unwrap()
                .confirmed();
        let devmode_plan = backend
            .transaction_with_confirmation(
                WriteOperation::Install {
                    packages: vec![devmode.clone()],
                },
                Some(&dev_confirmation),
            )
            .unwrap();
        println!("strict={strict_plan:?}");
        println!("classic={classic_plan:?}");
        println!("devmode={devmode_plan:?}");
        assert_eq!(strict_plan.command.args, ["install", "hello-world"]);
        assert_eq!(
            classic_plan.command.args,
            ["install", "classic-app", "--classic"]
        );
        assert_eq!(
            devmode_plan.command.args,
            ["install", "dev-app", "--channel=2/stable", "--devmode"]
        );
        assert_eq!(strict_plan.command.privilege, CommandPrivilege::Elevated);

        let dangerous = PackageIdentity::new(
            BackendId::Snap,
            PackageKind::Snap,
            PackageScope::System,
            NativePackageKey::new("/tmp/fixture.snap").unwrap(),
        );
        let local = PackageIdentity::new(
            BackendId::Snap,
            PackageKind::Snap,
            PackageScope::System,
            NativePackageKey::new("./fixture.snap").unwrap(),
        );
        assert_eq!(
            backend.transaction(WriteOperation::Install {
                packages: vec![dangerous]
            }),
            Err(SnapError::UnsupportedInstallSource)
        );
        assert_eq!(
            backend.transaction(WriteOperation::Install {
                packages: vec![local]
            }),
            Err(SnapError::UnsupportedInstallSource)
        );
        println!("dangerous=UnsupportedInstallSource");
        println!("local=UnsupportedInstallSource");

        let remove = backend
            .transaction(WriteOperation::Remove {
                packages: vec![strict.clone()],
            })
            .unwrap();
        assert_eq!(remove.command.args, ["remove", "hello-world"]);
        assert_eq!(remove.command.privilege, CommandPrivilege::Elevated);
        assert_eq!(backend.system_upgrade_plan().args, ["refresh"]);
        assert_eq!(
            backend.system_upgrade_plan().privilege,
            CommandPrivilege::Elevated
        );
    }

    #[test]
    fn classic_and_devmode_require_matching_confirmation_before_plan() {
        let backend = SnapBackend::from_paths("/snap");
        let classic = fixture_identity(
            "classic-app",
            "latest/stable",
            SnapConfinement::Classic,
            "acme",
        );
        assert!(matches!(
            backend.transaction(WriteOperation::Install {
                packages: vec![classic.clone()]
            }),
            Err(SnapError::ConfirmationRequired {
                confinement: SnapConfinement::Classic
            })
        ));
        let wrong =
            SnapInstallConfirmation::new("other", "latest/stable", SnapConfinement::Classic)
                .unwrap()
                .confirmed();
        assert_eq!(
            backend.transaction_with_confirmation(
                WriteOperation::Install {
                    packages: vec![classic]
                },
                Some(&wrong),
            ),
            Err(SnapError::ConfirmationMismatch { field: "publisher" })
        );
    }

    #[test]
    fn malformed_rows_and_untrusted_ids_are_rejected() {
        assert!(matches!(
            parse_find("Name Version Publisher Notes Summary\nmissing-fields\n"),
            Err(SnapError::MalformedRecord { .. })
        ));
        assert!(matches!(
            parse_info("summary: no name\nconfinement: strict\n"),
            Err(SnapError::MissingField { field: "name", .. })
        ));
        assert!(matches!(
            parse_info("name: foo\npublisher: acme\nchannels:\n  latest/stable: 1.0 (1) 1MB\n"),
            Err(SnapError::MissingField { field: "mode", .. })
        ));
        assert!(
            parse_refresh_list("All snaps up to date.\n")
                .unwrap()
                .is_empty()
        );
        for value in [
            "--help",
            "foo;touch",
            "foo@latest/stable#dangerous",
            "/tmp/x.snap",
        ] {
            assert!(SnapPackageId::parse(value).is_err(), "{value} was accepted");
        }
    }

    #[test]
    fn read_and_refresh_plans_are_user_or_elevated_as_declared() {
        let backend = SnapBackend::from_paths("/snap");
        assert_eq!(backend.search_plan("he").args, ["find", "he"]);
        assert_eq!(
            backend
                .details_plan(&PackageId::new("hello-world").unwrap())
                .unwrap()
                .args,
            ["info", "hello-world"]
        );
        assert_eq!(backend.updates_plan().args, ["refresh", "--list"]);
        assert_eq!(backend.refresh_plan().args, ["refresh"]);
        assert_eq!(backend.refresh_plan().privilege, CommandPrivilege::Elevated);
        assert_eq!(backend.list_plan().privilege, CommandPrivilege::User);
    }
}
