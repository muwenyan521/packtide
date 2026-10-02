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
        let key = key.trim();
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(PlatformError::MalformedOsRelease);
        }
        let value = parse_value(raw.trim())?;
        out.insert(key.to_string(), value);
    }
    Ok(out)
}

fn parse_value(raw: &str) -> Result<String, PlatformError> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    match raw.as_bytes()[0] {
        b'\'' => parse_single_quoted_value(raw),
        b'"' => parse_double_quoted_value(raw),
        _ => parse_unquoted_value(raw),
    }
}

fn parse_single_quoted_value(raw: &str) -> Result<String, PlatformError> {
    let rest = &raw[1..];
    let Some(end) = rest.find('\'') else {
        return Err(PlatformError::MalformedOsRelease);
    };
    if !rest[end + 1..].trim().is_empty() {
        return Err(PlatformError::MalformedOsRelease);
    }
    Ok(rest[..end].to_owned())
}

fn parse_double_quoted_value(raw: &str) -> Result<String, PlatformError> {
    let rest = &raw[1..];
    let mut value = String::new();
    let mut escaped = false;
    let mut end = None;
    for (index, c) in rest.char_indices() {
        if escaped {
            if matches!(c, '"' | '\\' | '$' | '`') {
                value.push(c);
            } else {
                value.push('\\');
                value.push(c);
            }
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => {
                end = Some(index);
                break;
            }
            other => value.push(other),
        }
    }
    let Some(end) = end else {
        return Err(PlatformError::MalformedOsRelease);
    };
    if escaped || !rest[end + 1..].trim().is_empty() {
        return Err(PlatformError::MalformedOsRelease);
    }
    Ok(value)
}

fn parse_unquoted_value(raw: &str) -> Result<String, PlatformError> {
    let mut value = String::new();
    let mut escaped = false;
    for c in raw.chars() {
        if escaped {
            value.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c.is_ascii_whitespace() || matches!(c, '\'' | '"' | '$' | '`' | ';') {
            return Err(PlatformError::MalformedOsRelease);
        }
        value.push(c);
    }
    if escaped {
        return Err(PlatformError::MalformedOsRelease);
    }
    Ok(value)
}

fn backend_for_id(id: &str) -> Option<NativeBackend> {
    match id.to_ascii_lowercase().as_str() {
        "arch" | "manjaro" | "endeavouros" | "endeavour" => Some(NativeBackend::Pacman),
        "debian" | "ubuntu" | "linuxmint" | "mint" | "pop" | "pop_os" | "elementary" => {
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
    use std::cell::RefCell;

    fn f(id: &str) -> String {
        format!("ID='{id}'\nNAME='A \\\"name\\\"'\n")
    }

    #[test]
    fn maps_supported_distribution_ids() {
        for (id, expected) in [
            ("arch", NativeBackend::Pacman),
            ("manjaro", NativeBackend::Pacman),
            ("endeavouros", NativeBackend::Pacman),
            ("debian", NativeBackend::Apt),
            ("ubuntu", NativeBackend::Apt),
            ("linuxmint", NativeBackend::Apt),
            ("mint", NativeBackend::Apt),
            ("pop", NativeBackend::Apt),
            ("fedora", NativeBackend::Dnf),
            ("rhel", NativeBackend::Dnf),
            ("rocky", NativeBackend::Dnf),
            ("almalinux", NativeBackend::Dnf),
            ("opensuse", NativeBackend::Zypper),
            ("opensuse-leap", NativeBackend::Zypper),
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
            backend_from_os_release("ID='ubuntu'\nID_LIKE=\"fedora debian\"\n"),
            Ok(NativeBackend::Apt)
        );
        assert_eq!(
            backend_from_os_release("ID=unknown\nID_LIKE='fedora debian'\n"),
            Ok(NativeBackend::Dnf)
        );
    }

    #[test]
    fn parses_single_double_and_unquoted_escapes() {
        let p = parse_os_release(
            r#"SINGLE='A \"quoted\"'
DOUBLE="A \"quoted\"\nline"
ESCAPED="dollar\$ tick\` slash\\ unknown\q"
ID_LIKE=fedora\ debian
"#,
        )
        .unwrap();
        assert_eq!(p["SINGLE"], r#"A \"quoted\""#);
        assert_eq!(p["DOUBLE"], "A \"quoted\"\\nline");
        assert_eq!(p["ESCAPED"], r#"dollar$ tick` slash\ unknown\q"#);
        assert_eq!(p["ID_LIKE"], "fedora debian");
    }

    #[test]
    fn malformed_values_and_unknown_ids_are_rejected() {
        assert_eq!(
            backend_from_os_release("ID=wat\n"),
            Err(PlatformError::UnsupportedDistribution)
        );
        assert!(parse_os_release("ID=\"unterminated\n").is_err());
        assert!(parse_os_release("ID='unterminated\n").is_err());
        assert!(parse_os_release("ID=\"one\" \"two\"\n").is_err());
        assert!(parse_os_release("ID=foo$bar\n").is_err());
    }

    #[test]
    fn path_does_not_change_family() {
        let err = detect_native_backend("ID=ubuntu\n", |_| true);
        assert_eq!(err, Ok(NativeBackend::Apt));
    }

    #[test]
    fn required_tools_follow_id_instead_of_path_decoys() {
        let requested = RefCell::new(Vec::new());
        let detected = detect_native_backend("ID='ubuntu'\nID_LIKE='fedora'\n", |command| {
            requested
                .borrow_mut()
                .push(command.to_string_lossy().into_owned());
            matches!(command.to_str(), Some("apt-get" | "dpkg-query"))
        });
        assert_eq!(detected, Ok(NativeBackend::Apt));
        assert_eq!(requested.into_inner(), ["apt-get", "dpkg-query"]);

        let missing_apt =
            detect_native_backend("ID=ubuntu\n", |command| command == OsStr::new("dnf"));
        assert_eq!(missing_apt, Err(PlatformError::MissingTool));
    }

    #[test]
    fn arch_id_selects_pacman_when_native_tool_is_available() {
        let detected =
            detect_native_backend("ID=arch\n", |command| command == OsStr::new("pacman"));
        assert_eq!(detected, Ok(NativeBackend::Pacman));
    }

    #[test]
    fn ubuntu_id_does_not_select_dnf_decoy() {
        let detected = detect_native_backend("ID=ubuntu\nID_LIKE=fedora\n", |command| {
            command == OsStr::new("dnf")
        });
        assert_eq!(detected, Err(PlatformError::MissingTool));
    }
}
