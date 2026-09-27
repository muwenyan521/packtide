use system_tools_core::PackageSource;

pub(crate) fn source_color(source: PackageSource) -> &'static str {
    match source {
        PackageSource::Pacman => "34",
        PackageSource::Aur => "35",
        PackageSource::Flatpak => "36",
    }
}

pub(crate) fn strip_ansi(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut escape = false;
    for ch in value.chars() {
        if escape {
            if ch.is_ascii_alphabetic() {
                escape = false;
            }
        } else if ch == '\x1b' {
            escape = true;
        } else {
            output.push(ch);
        }
    }
    output
}
