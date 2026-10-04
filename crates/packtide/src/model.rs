pub(crate) use system_tools_core::PackageSource as UpdateSource;
use system_tools_core::{
    BackendId, NativePackageKey, PackageIdentity, PackageKind, PackageScope, PackageSource,
};

#[derive(Debug)]
pub(crate) struct PackageUpdate {
    pub(crate) source: UpdateSource,
    pub(crate) identity: PackageIdentity,
    pub(crate) name: String,
    pub(crate) current: Option<String>,
    pub(crate) candidate: Option<String>,
    pub(crate) display: String,
}

impl PackageUpdate {
    pub(crate) fn legacy(
        source: PackageSource,
        name: String,
        candidate: Option<String>,
        display: String,
    ) -> Self {
        let identity = PackageIdentity::new(
            source.backend_for_source(),
            source.default_kind(),
            source.default_scope(),
            NativePackageKey::new(name.clone()).expect("legacy update package name"),
        );
        Self::from_identity(source, identity, name, candidate, display)
    }

    pub(crate) fn from_identity(
        source: PackageSource,
        identity: PackageIdentity,
        name: String,
        candidate: Option<String>,
        display: String,
    ) -> Self {
        let display = identity.display_name.clone().unwrap_or(display);
        let (current, candidate) = parse_versions(&name, &display, candidate);
        Self {
            source,
            identity,
            name,
            current,
            candidate,
            display,
        }
    }

    pub(crate) fn with_versions(
        source: PackageSource,
        identity: PackageIdentity,
        name: String,
        current: Option<String>,
        candidate: Option<String>,
        display: String,
    ) -> Self {
        let display = identity.display_name.clone().unwrap_or(display);
        Self {
            source,
            identity,
            name,
            current,
            candidate,
            display,
        }
    }
}

fn parse_versions(
    name: &str,
    display: &str,
    explicit_candidate: Option<String>,
) -> (Option<String>, Option<String>) {
    let parts = display.split_whitespace().collect::<Vec<_>>();
    let arrow = parts.iter().position(|part| *part == "->");
    let current = arrow
        .and_then(|index| index.checked_sub(1))
        .filter(|index| parts[*index] != name)
        .map(|index| parts[index].to_owned());
    let candidate = explicit_candidate.or_else(|| {
        arrow
            .and_then(|index| parts.get(index + 1).copied())
            .or_else(|| {
                parts
                    .last()
                    .copied()
                    .filter(|value| *value != name && *value != "->")
            })
            .map(str::to_owned)
    });
    (current, candidate)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackageRecord {
    pub(crate) backend: BackendId,
    pub(crate) kind: PackageKind,
    pub(crate) scope: PackageScope,
    pub(crate) native_key: NativePackageKey,
    pub(crate) origin: Option<String>,
    pub(crate) display_name: Option<String>,
    pub(crate) source: PackageSource,
    pub(crate) repository: Option<String>,
    pub(crate) name: String,
    pub(crate) listing: PackageListing,
    pub(crate) installed: bool,
}

impl PackageRecord {
    pub(crate) fn legacy(
        source: PackageSource,
        repository: Option<String>,
        name: String,
        listing: PackageListing,
        installed: bool,
    ) -> Self {
        let backend = source.backend_for_source();
        let scope = crate::source_metadata::legacy_scope(source, repository.as_deref());
        let native_key = NativePackageKey::new(name.clone()).expect("legacy package name");
        let (origin, display_name) = match &listing {
            PackageListing::Flatpak { app_name, origin } => (
                (!origin.is_empty()).then(|| origin.clone()),
                (!app_name.is_empty()).then(|| app_name.clone()),
            ),
            PackageListing::Version(_) => (None, None),
        };
        Self {
            backend,
            kind: source.default_kind(),
            scope,
            native_key,
            origin,
            display_name,
            source,
            repository,
            name,
            listing,
            installed,
        }
    }

    pub(crate) fn from_identity(
        identity: PackageIdentity,
        repository: Option<String>,
        name: String,
        listing: PackageListing,
        installed: bool,
    ) -> Self {
        Self {
            backend: identity.backend,
            kind: identity.kind,
            scope: identity.scope,
            native_key: identity.native_key,
            origin: identity.origin,
            display_name: identity.display_name,
            source: identity.backend.package_source(),
            repository,
            name,
            listing,
            installed,
        }
    }
}

impl PackageRecord {
    pub(crate) fn identity(&self) -> PackageIdentity {
        let mut identity =
            PackageIdentity::new(self.backend, self.kind, self.scope, self.native_key.clone());
        if let Some(origin) = &self.origin {
            identity = identity.with_origin(origin.clone());
        }
        if let Some(display_name) = &self.display_name {
            identity = identity.with_display_name(display_name.clone());
        }
        identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PackageListing {
    Version(String),
    Flatpak { app_name: String, origin: String },
}

#[allow(dead_code)]
pub(crate) fn parse_updates(source: PackageSource, text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_owned();
            Some(PackageUpdate::legacy(
                source,
                name,
                parts.next_back().map(str::to_owned),
                line.to_owned(),
            ))
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn parse_flatpak(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_owned();
            Some(PackageUpdate::legacy(
                PackageSource::Flatpak,
                name,
                parts.next().map(str::to_owned),
                line.to_owned(),
            ))
        })
        .collect()
}

pub(crate) fn parse_cached_updates(contents: &str) -> Vec<PackageUpdate> {
    contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let first = fields.next()?;
            if let (Some(scope), Some(native_key), Some(source), Some(name)) = (
                fields.next().and_then(PackageScope::parse),
                fields
                    .next()
                    .and_then(|value| NativePackageKey::new(value).ok()),
                fields.next().and_then(PackageSource::parse),
                fields.next(),
            ) {
                let backend = BackendId::parse(first)?;
                let identity =
                    PackageIdentity::new(backend, backend.default_kind(), scope, native_key);
                let rest = fields.collect::<Vec<_>>();
                let (current, candidate, display) = match rest.as_slice() {
                    [current, candidate, display] => (
                        (!current.is_empty()).then(|| (*current).to_owned()),
                        (!candidate.is_empty()).then(|| (*candidate).to_owned()),
                        (*display).to_owned(),
                    ),
                    [display] => {
                        let (current, candidate) = parse_versions(name, display, None);
                        (current, candidate, (*display).to_owned())
                    }
                    _ => return None,
                };
                return Some(PackageUpdate::with_versions(
                    source,
                    identity,
                    name.to_owned(),
                    current,
                    candidate,
                    display,
                ));
            }

            let (source, display) = line.split_once('\t')?;
            let mut parts = display.split_whitespace();
            let source = PackageSource::parse(source)?;
            let name = parts.next()?.to_owned();
            Some(PackageUpdate::legacy(
                source,
                name,
                parts.next_back().map(str::to_owned),
                display.to_owned(),
            ))
        })
        .collect()
}

pub(crate) fn render_update_rows(updates: &[PackageUpdate]) -> String {
    use std::fmt::Write as _;

    let capacity = updates
        .iter()
        .map(|item| item.name.len() + item.display.len() + 32)
        .sum();
    let mut rows = String::with_capacity(capacity);
    for (index, item) in updates.iter().enumerate() {
        if index > 0 {
            rows.push('\n');
        }
        let color = crate::ui::source_color(item.source);
        let source_label = crate::locale::source_label(crate::locale::current(), item.source);
        let display = item.display_for_row();
        write!(
            rows,
            "\x1b[{color}m[{:<7}]\x1b[0m\t{}\t{}",
            source_label, item.name, display
        )
        .expect("writing update row to String cannot fail");
    }
    rows
}

impl PackageUpdate {
    fn display_for_row(&self) -> String {
        if self.display.contains("->") {
            return self.display.clone();
        }
        match (&self.current, &self.candidate) {
            (Some(current), Some(candidate)) => {
                format!("{} {} -> {}", self.name, current, candidate)
            }
            (None, Some(candidate)) if !self.display.contains(candidate) => {
                format!("{} {}", self.display, candidate)
            }
            _ => self.display.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageUpdate, render_update_rows};
    use system_tools_core::{
        BackendId, NativePackageKey, PackageIdentity, PackageKind, PackageScope, PackageSource,
    };

    #[test]
    fn update_rows_use_localized_source_labels() {
        let rows = render_update_rows(&[
            PackageUpdate {
                source: PackageSource::Pacman,
                identity: PackageIdentity::new(
                    BackendId::Pacman,
                    PackageKind::System,
                    PackageScope::System,
                    NativePackageKey::new("bash").unwrap(),
                ),
                name: "bash".to_owned(),
                current: Some("5.2".to_owned()),
                candidate: Some("5.3".to_owned()),
                display: "bash 5.3".to_owned(),
            },
            PackageUpdate {
                source: PackageSource::Aur,
                identity: PackageIdentity::new(
                    BackendId::Paru,
                    PackageKind::Aur,
                    PackageScope::User,
                    NativePackageKey::new("tool").unwrap(),
                ),
                name: "tool".to_owned(),
                current: Some("0.9".to_owned()),
                candidate: Some("1.0".to_owned()),
                display: "tool 1.0".to_owned(),
            },
            PackageUpdate {
                source: PackageSource::Flatpak,
                identity: PackageIdentity::new(
                    BackendId::Flatpak,
                    PackageKind::Flatpak,
                    PackageScope::User,
                    NativePackageKey::new("org.example.App").unwrap(),
                ),
                name: "org.example.App".to_owned(),
                current: None,
                candidate: Some("2.0".to_owned()),
                display: "org.example.App 2.0".to_owned(),
            },
        ]);
        let plain = crate::ui::strip_ansi(&rows);
        let lang = crate::locale::current();
        for source in [
            PackageSource::Pacman,
            PackageSource::Aur,
            PackageSource::Flatpak,
        ] {
            let label = crate::locale::source_label(lang, source);
            assert!(plain.contains(&format!("[{label}")), "missing {label}");
        }
    }
}
