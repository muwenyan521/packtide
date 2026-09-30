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
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn strips_csi_and_osc_sequences_without_losing_text() {
        let value = "\x1b[1;38;5;45mName\x1b[0m : \x1b]8;;https://example.test\x07bash\x1b]8;;\x07";
        assert_eq!(strip_ansi(value), "Name : bash");
    }
}
