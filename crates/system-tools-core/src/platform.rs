//! Host distribution detection and native package-manager selection.
use std::{collections::BTreeMap, ffi::OsStr, fmt, fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeBackend {
    Pacman,
    Apt,
    Dnf,
    Zypper,
    Apk,
    Xbps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformError {
    MalformedOsRelease,
    UnsupportedDistribution,
    MissingTool,
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MalformedOsRelease => "malformed os-release",
            Self::UnsupportedDistribution => "unsupported distribution",
            Self::MissingTool => "required package manager is unavailable",
        })
    }
}
impl std::error::Error for PlatformError {}

/// Parse the freedesktop os-release format. Values may be unquoted or quoted;
/// quoted backslash escapes are decoded without invoking a shell.
pub fn parse_os_release(input: &str) -> Result<BTreeMap<String, String>, PlatformError> {
    let mut out = BTreeMap::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            return Err(PlatformError::MalformedOsRelease);
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(PlatformError::MalformedOsRelease);
        }
        let value = parse_value(raw.trim())?;
        out.insert(key.to_string(), value);
    }
    Ok(out)
}

fn parse_value(raw: &str) -> Result<String, PlatformError> {
    if let Some(rest) = raw.strip_prefix('"') {
        let mut value = String::new();
        let mut escaped = false;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            if escaped {
                value.push(match c {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
                continue;
            }
            match c {
                '\\' => escaped = true,
                '"' => {
                    end = Some(i);
                    break;
                }
                other => value.push(other),
            }
        }
        let Some(end) = end else {
            return Err(PlatformError::MalformedOsRelease);
        };
        if !rest[end + 1..].trim().is_empty() {
            return Err(PlatformError::MalformedOsRelease);
        }
        if escaped {
            return Err(PlatformError::MalformedOsRelease);
        }
        Ok(value)
    } else {
        if raw
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b == b'"' || b == b'\'')
        {
            return Err(PlatformError::MalformedOsRelease);
        }
        Ok(raw.to_string())
    }
}

fn backend_for_id(id: &str) -> Option<NativeBackend> {
    match id.to_ascii_lowercase().as_str() {
        "arch" | "manjaro" | "endeavouros" | "endeavour" => Some(NativeBackend::Pacman),
        "debian" | "ubuntu" | "linuxmint" | "pop" | "pop_os" | "elementary" => {
            Some(NativeBackend::Apt)
        }
        "fedora" | "rhel" | "redhat" | "centos" | "rocky" | "almalinux" | "alma" | "ol" => {
            Some(NativeBackend::Dnf)
        }
        "opensuse" | "opensuse-leap" | "sles" | "suse" => Some(NativeBackend::Zypper),
        "alpine" => Some(NativeBackend::Apk),
        "void" => Some(NativeBackend::Xbps),
        _ => None,
    }
}

pub fn backend_from_os_release(input: &str) -> Result<NativeBackend, PlatformError> {
    let fields = parse_os_release(input)?;
    if let Some(id) = fields.get("ID")
        && let Some(backend) = backend_for_id(id)
    {
        return Ok(backend);
    }
    if let Some(like) = fields.get("ID_LIKE") {
        for id in like.split_ascii_whitespace() {
            if let Some(backend) = backend_for_id(id) {
                return Ok(backend);
            }
        }
    }
    Err(PlatformError::UnsupportedDistribution)
}

impl NativeBackend {
    pub const fn required_commands(self) -> &'static [&'static str] {
        match self {
            Self::Pacman => &["pacman"],
            Self::Apt => &["apt-get", "dpkg-query"],
            Self::Dnf => &["dnf"],
            Self::Zypper => &["zypper"],
            Self::Apk => &["apk"],
            Self::Xbps => &["xbps-query", "xbps-install"],
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
            Self::Apk => "apk",
            Self::Xbps => "xbps",
        }
    }
}

pub fn detect_native_backend(
    input: &str,
    command_available: impl Fn(&OsStr) -> bool,
) -> Result<NativeBackend, PlatformError> {
    let backend = backend_from_os_release(input)?;
    if backend
        .required_commands()
        .iter()
        .all(|cmd| command_available(OsStr::new(cmd)))
    {
        Ok(backend)
    } else {
        Err(PlatformError::MissingTool)
    }
}

pub fn detect_native_backend_from_path(
    input: &str,
    path: Option<&OsStr>,
) -> Result<NativeBackend, PlatformError> {
    let resolver = crate::ExecutableResolver::from_path(path);
    detect_native_backend(input, |cmd| resolver.resolve(cmd).is_some())
}

pub fn detect_native_backend_from_file(
    path: impl AsRef<Path>,
) -> Result<NativeBackend, PlatformError> {
    let input = fs::read_to_string(path).map_err(|_| PlatformError::UnsupportedDistribution)?;
    detect_native_backend_from_path(&input, std::env::var_os("PATH").as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn f(id: &str) -> String {
        format!("ID={id}\nNAME=\"A \\\"name\\\"\"\n")
    }
    #[test]
    fn maps_table() {
        for (id, expected) in [
            ("arch", NativeBackend::Pacman),
            ("manjaro", NativeBackend::Pacman),
            ("debian", NativeBackend::Apt),
            ("ubuntu", NativeBackend::Apt),
            ("linuxmint", NativeBackend::Apt),
            ("pop", NativeBackend::Apt),
            ("fedora", NativeBackend::Dnf),
            ("rhel", NativeBackend::Dnf),
            ("rocky", NativeBackend::Dnf),
            ("alma", NativeBackend::Dnf),
            ("opensuse", NativeBackend::Zypper),
            ("sles", NativeBackend::Zypper),
            ("alpine", NativeBackend::Apk),
            ("void", NativeBackend::Xbps),
        ] {
            assert_eq!(backend_from_os_release(&f(id)), Ok(expected));
        }
    }
    #[test]
    fn id_precedes_like_and_like_ordered() {
        assert_eq!(
            backend_from_os_release("ID=ubuntu\nID_LIKE=\"fedora debian\"\n"),
            Ok(NativeBackend::Apt)
        );
        assert_eq!(
            backend_from_os_release("ID=unknown\nID_LIKE=\"fedora debian\"\n"),
            Ok(NativeBackend::Dnf)
        );
    }
    #[test]
    fn quoted_escapes() {
        let p = parse_os_release("NAME=\"A \\\"quoted\\\"\\nline\"\n").unwrap();
        assert_eq!(p["NAME"], "A \"quoted\"\nline");
    }
    #[test]
    fn malformed_and_unknown() {
        assert_eq!(
            backend_from_os_release("ID=wat\n"),
            Err(PlatformError::UnsupportedDistribution)
        );
        assert!(parse_os_release("ID=\"unterminated\n").is_err());
    }
    #[test]
    fn path_does_not_change_family() {
        let err = detect_native_backend("ID=ubuntu\n", |_| true);
        assert_eq!(err, Ok(NativeBackend::Apt));
    }
}
