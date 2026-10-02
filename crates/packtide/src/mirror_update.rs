use anyhow::{Result, bail};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use system_tools_core::run_capture;

mod transaction;
use transaction::{MirrorBackup, PrivilegedCommandRunner, SystemPrivilegedCommandRunner};

#[cfg(test)]
mod tests;

pub fn run(country: Option<String>) -> Result<()> {
    crate::app::require_command_for(
        "reflector",
        "capability.refresh",
        "the mirror list update",
        false,
    )?;
    let target = country.or_else(detect_country);
    let paths = MirrorPaths {
        mirrorlist: PathBuf::from("/etc/pacman.d/mirrorlist"),
        backup_dir: env::var_os("XDG_CACHE_HOME")
            .map(|path| PathBuf::from(path).join("packtide/mirror-update"))
            .or_else(|| {
                env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".cache/packtide/mirror-update"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/packtide-mirror-update")),
    };
    let reflector = SystemReflector;
    let privileged = SystemPrivilegedCommandRunner;
    MirrorUpdate {
        country: target,
        paths,
        reflector: &reflector,
        privileged: &privileged,
    }
    .execute()
}

struct MirrorPaths {
    mirrorlist: PathBuf,
    backup_dir: PathBuf,
}

trait ReflectorProvider {
    fn countries(&self) -> Result<String>;
    fn mirrors(&self, country: Option<&str>) -> Result<Option<String>>;
}

struct SystemReflector;

impl ReflectorProvider for SystemReflector {
    fn countries(&self) -> Result<String> {
        Ok(run_capture("reflector", &["--list-countries"], false)?.stdout)
    }

    fn mirrors(&self, country: Option<&str>) -> Result<Option<String>> {
        let owned = match country {
            Some(country) => vec![
                "--protocol".to_owned(),
                "https".to_owned(),
                "--sort".to_owned(),
                "rate".to_owned(),
                "--fastest".to_owned(),
                "10".to_owned(),
                "--age".to_owned(),
                "12".to_owned(),
                "--country".to_owned(),
                country.to_owned(),
            ],
            None => vec![
                "--protocol".to_owned(),
                "https".to_owned(),
                "--sort".to_owned(),
                "rate".to_owned(),
                "--fastest".to_owned(),
                "10".to_owned(),
                "--latest".to_owned(),
                "50".to_owned(),
                "--download-timeout".to_owned(),
                "5".to_owned(),
            ],
        };
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let output = run_capture("reflector", &args, true)?;
        Ok((output.status.success() && !output.stdout.trim().is_empty()).then_some(output.stdout))
    }
}

struct MirrorUpdate<'a, R, P> {
    country: Option<String>,
    paths: MirrorPaths,
    reflector: &'a R,
    privileged: &'a P,
}

impl<R, P> MirrorUpdate<'_, R, P>
where
    R: ReflectorProvider,
    P: PrivilegedCommandRunner,
{
    fn execute(&self) -> Result<()> {
        let backup = MirrorBackup::new(&self.paths.mirrorlist, &self.paths.backup_dir)?;
        let countries = self.reflector.countries()?.to_ascii_lowercase();
        if let Some(selected) = self.country.as_deref() {
            let recognized = selected.len() <= 2
                || countries
                    .lines()
                    .any(|line| line.trim().eq_ignore_ascii_case(selected));
            if !recognized {
                eprintln!(
                    "\x1b[1;33m[WARN]\x1b[0m country not recognized by reflector: {selected}; using global fallback"
                );
            } else if let Some(output) = self.reflector.mirrors(Some(selected))? {
                self.print_transaction_summary();
                backup.install(&output, self.privileged)?;
                println!(
                    "\x1b[1;32m[OK]\x1b[0m mirrorlist updated; backup: {}",
                    backup.backup_path().display()
                );
                return Ok(());
            } else if let Some(region) = fallback_region(selected) {
                eprintln!(
                    "\x1b[1;33m[WARN]\x1b[0m regional mirror attempt failed; trying {region}"
                );
                if let Some(output) = self.reflector.mirrors(Some(&region))? {
                    self.print_transaction_summary();
                    backup.install(&output, self.privileged)?;
                    println!(
                        "\x1b[1;32m[OK]\x1b[0m mirrorlist updated regionally; backup: {}",
                        backup.backup_path().display()
                    );
                    return Ok(());
                }
            }
        }
        if let Some(output) = self.reflector.mirrors(None)? {
            self.print_transaction_summary();
            backup.install(&output, self.privileged)?;
            println!(
                "\x1b[1;32m[OK]\x1b[0m mirrorlist updated globally; backup: {}",
                backup.backup_path().display()
            );
            return Ok(());
        }
        bail!("all mirror update attempts failed; original mirrorlist restored")
    }

    fn print_transaction_summary(&self) {
        println!(
            "\x1b[1;34mtransaction:\x1b[0m action=mirror-update helper=reflector privilege=sudo targets={}",
            self.paths.mirrorlist.display()
        );
    }
}

pub(crate) fn fallback_region(country: &str) -> Option<String> {
    let countries =
        if country.contains("China") || country.contains("Hong Kong") || country.contains("Taiwan")
        {
            "China,Hong Kong,Taiwan,Japan,Singapore"
        } else if country.contains("Japan") || country.contains("Korea") {
            "Japan,South Korea,Taiwan,Hong Kong"
        } else if country.contains("Singapore") || country.contains("Malaysia") {
            "Singapore,Malaysia,Thailand,Indonesia,China"
        } else if country.contains("United States") || country.contains("Canada") {
            "United States,Canada"
        } else if country.contains("Germany")
            || country.contains("France")
            || country.contains("United Kingdom")
        {
            "Germany,France,United Kingdom,Netherlands"
        } else {
            return None;
        };
    Some(countries.to_owned())
}

fn detect_country() -> Option<String> {
    let timezone = Command::new("timedatectl")
        .args(["show", "-p", "Timezone", "--value"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default();
    let country = if timezone.contains("Shanghai") || timezone.contains("Chongqing") {
        "China"
    } else if timezone.contains("Tokyo") || timezone.contains("Osaka") {
        "Japan"
    } else if timezone.contains("Singapore") {
        "Singapore"
    } else if timezone.contains("Seoul") {
        "South Korea"
    } else if timezone.contains("London") {
        "United Kingdom"
    } else if timezone.contains("Berlin") {
        "Germany"
    } else {
        ""
    };
    if !country.is_empty() {
        return Some(country.to_owned());
    }
    let response = ureq::get("https://ipapi.co/country_name/")
        .call()
        .ok()?
        .into_body()
        .read_to_string()
        .ok()?;
    let response = response.trim();
    (!response.is_empty() && !response.contains("error")).then(|| response.to_owned())
}
