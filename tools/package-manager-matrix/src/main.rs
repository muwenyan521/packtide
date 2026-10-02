use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_LOCK: &str = "tests/package-managers/images.lock";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Clone, Debug)]
struct Image {
    name: String,
    registry: String,
    tag: String,
    digest: String,
    arch: String,
    manager: String,
    version: String,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "doctor".into());
    match command.as_str() {
        "doctor" => doctor(),
        "list-images" => list_images(Path::new(DEFAULT_LOCK)),
        "probe-alpine" => {
            let mut image = None;
            let mut evidence = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--image" => image = Some(args.next().context("--image needs a value")?),
                    "--evidence" => {
                        evidence = Some(PathBuf::from(
                            args.next().context("--evidence needs a value")?,
                        ))
                    }
                    value => bail!("unknown probe-alpine argument: {value}"),
                }
            }
            probe_alpine(image.as_deref(), evidence.as_deref())
        }
        "all" => {
            let mut evidence = None;
            while let Some(arg) = args.next() {
                if arg == "--evidence" {
                    evidence = Some(PathBuf::from(
                        args.next().context("--evidence needs a value")?,
                    ));
                } else {
                    bail!("unknown all argument: {arg}");
                }
            }
            probe_alpine(None, evidence.as_deref())
        }
        _ => bail!("usage: package-manager-matrix <doctor|list-images|probe-alpine|all> [options]"),
    }
}

fn doctor() -> Result<()> {
    let podman = command_version("podman");
    let qemu = command_version("qemu-system-x86_64");
    let kvm = Path::new("/dev/kvm").exists();
    let cloud_init = command_version("cloud-init");
    println!(
        "{{\"podman\":{},\"qemu\":{},\"kvm\":{},\"cloud_init\":{}}}",
        json_string(podman.as_deref().unwrap_or("MISSING")),
        json_string(qemu.as_deref().unwrap_or("MISSING")),
        kvm,
        json_string(cloud_init.as_deref().unwrap_or("MISSING"))
    );
    if podman.is_none() || qemu.is_none() || !kvm {
        bail!("doctor failed: Podman, QEMU and /dev/kvm are required");
    }
    Ok(())
}

fn list_images(path: &Path) -> Result<()> {
    let images = read_lock(path)?;
    for image in images {
        validate_digest(&image.digest)?;
        verify_manifest(&image)?;
        println!(
            "{{\"name\":{},\"registry\":{},\"tag\":{},\"manifest_digest\":{},\"architecture\":{},\"package_manager\":{},\"version\":{}}}",
            json_string(&image.name),
            json_string(&image.registry),
            json_string(&image.tag),
            json_string(&image.digest),
            json_string(&image.arch),
            json_string(&image.manager),
            json_string(&image.version)
        );
    }
    Ok(())
}

fn verify_manifest(image: &Image) -> Result<()> {
    let reference = format!("{}:{}", image.registry, image.tag);
    let output = Command::new("podman")
        .args(["manifest", "inspect", &reference])
        .output()
        .with_context(|| format!("inspect manifest {reference}"))?;
    if !output.status.success() {
        bail!(
            "manifest inspect failed for {}: {}",
            reference,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let body = String::from_utf8_lossy(&output.stdout);
    if body.contains(&image.digest) {
        return Ok(());
    }
    // Podman reports a single-image manifest without an index digest. Verify
    // that case through the OCI registry inspector instead of trusting a tag.
    let skopeo_ref = format!("docker://{reference}");
    let inspected = Command::new("skopeo")
        .args(["inspect", &skopeo_ref])
        .output()
        .with_context(|| format!("inspect single-image manifest {reference}"))?;
    if !inspected.status.success()
        || !String::from_utf8_lossy(&inspected.stdout).contains(&image.digest)
    {
        bail!("manifest inspect did not resolve locked digest {reference}");
    }
    Ok(())
}

fn probe_alpine(requested: Option<&str>, evidence_path: Option<&Path>) -> Result<()> {
    let images = read_lock(Path::new(DEFAULT_LOCK))?;
    let image = images
        .iter()
        .find(|i| i.name == "alpine")
        .context("alpine image missing from lock")?;
    let ref_name = requested.unwrap_or(&image.digest);
    if let Err(err) = validate_digest(ref_name) {
        let json = format!(
            "{{\"scenario\":\"alpine-noop\",\"image\":{},\"status\":1,\"error\":{},\"cleanup\":true}}",
            json_string(ref_name),
            json_string(&format!("invalid image digest: {err}"))
        );
        if let Some(path) = evidence_path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, format!("{json}\n"))?;
        }
        println!("{json}");
        bail!("invalid image digest (expected sha256:<64 hex>)");
    }
    let full_ref = format!("{}@{}", image.registry, ref_name);
    let name = format!("pm-matrix-{}-{}", std::process::id(), unix_nanos());
    let mut command = Command::new("podman");
    command
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "--network",
            "none",
            &full_ref,
            "apk",
            "--print-arch",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let result = run_bounded(command, COMMAND_TIMEOUT);
    let cleanup = cleanup_check(&name);
    let (status, stdout, stderr, error) = match result {
        Ok(output) => (
            output.status.code().unwrap_or(1),
            output.stdout,
            output.stderr,
            None,
        ),
        Err(err) => (1, String::new(), String::new(), Some(err.to_string())),
    };
    let json = format!(
        "{{\"scenario\":\"alpine-noop\",\"image\":{},\"manifest_digest\":{},\"architecture\":{},\"package_manager\":{},\"package_manager_version\":{},\"status\":{},\"elapsed_ms\":{},\"stdout\":{},\"stderr\":{},\"error\":{},\"cleanup\":{}}}",
        json_string(&full_ref),
        json_string(&image.digest),
        json_string(&image.arch),
        json_string(&image.manager),
        json_string(&image.version),
        status,
        started.elapsed().as_millis(),
        json_string(stdout.trim()),
        json_string(stderr.trim()),
        error
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        cleanup
    );
    if let Some(path) = evidence_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{json}\n"))?;
    }
    println!("{json}");
    if status != 0 {
        bail!("Alpine probe failed with status {status}");
    }
    Ok(())
}

struct Output {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_bounded(mut command: Command, timeout: Duration) -> Result<Output> {
    let mut child = command.spawn().context("spawn bounded command")?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return collect_output(child, status);
        }
        if start.elapsed() >= timeout {
            terminate(&mut child);
            bail!("command exceeded {}s timeout", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn collect_output(child: Child, status: ExitStatus) -> Result<Output> {
    let output = child.wait_with_output()?;
    Ok(Output {
        status,
        stdout: String::from_utf8_lossy(&output.stdout).into(),
        stderr: String::from_utf8_lossy(&output.stderr).into(),
    })
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn cleanup_check(name: &str) -> bool {
    Command::new("podman")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .map(|o| {
            !String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|line| line.trim() == name)
        })
        .unwrap_or(false)
}

fn read_lock(path: &Path) -> Result<Vec<Image>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("read image lock {}", path.display()))?;
    let mut images = Vec::new();
    let mut current = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix("[[image.")
            .and_then(|s| s.strip_suffix("]]"))
        {
            if let Some(image) = current.take() {
                images.push(image);
            }
            current = Some(Image {
                name: name.into(),
                registry: String::new(),
                tag: String::new(),
                digest: String::new(),
                arch: String::new(),
                manager: String::new(),
                version: String::new(),
            });
            continue;
        }
        let (key, value) = line.split_once('=').context("invalid image lock line")?;
        let value = value.trim().trim_matches('"');
        let image = current
            .as_mut()
            .context("image field before image header")?;
        match key.trim() {
            "registry" => image.registry = value.into(),
            "tag" => image.tag = value.into(),
            "manifest_digest" => image.digest = value.into(),
            "architecture" => image.arch = value.into(),
            "package_manager" => image.manager = value.into(),
            "version" => image.version = value.into(),
            _ => {}
        }
    }
    if let Some(image) = current {
        images.push(image);
    }
    if images.is_empty() {
        bail!("image lock is empty");
    }
    Ok(images)
}

fn validate_digest(value: &str) -> Result<()> {
    let hex = value
        .strip_prefix("sha256:")
        .context("digest must start with sha256:")?;
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("invalid digest: {value}");
    }
    Ok(())
}

fn command_version(command: &str) -> Option<String> {
    let output = Command::new(command).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut version = String::from_utf8_lossy(&output.stdout).into_owned();
    if version.trim().is_empty() {
        version = String::from_utf8_lossy(&output.stderr).into_owned();
    }
    Some(version.trim().to_string())
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}
fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digest_validation_rejects_bad_values() {
        assert!(validate_digest("sha256:bad").is_err());
        assert!(validate_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
    }
    #[test]
    fn lock_has_required_alpine_fields() {
        let images = read_lock(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/package-managers/images.lock")
                .as_path(),
        )
        .unwrap();
        let alpine = images.iter().find(|i| i.name == "alpine").unwrap();
        assert!(!alpine.registry.is_empty());
        assert!(!alpine.version.is_empty());
    }
    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(json_string("line\nq\t\u{0001}"), "\"line\\nq\\t\\u0001\"");
        assert!(!json_string("q\n").contains('\n'));
    }
}
