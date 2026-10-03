use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_LOCK: &str = "tests/package-managers/images.lock";
const DEFAULT_VM_LOCK: &str = "tests/package-managers/ubuntu-cloud-image.lock";
const BREW_IMAGE: &str = "docker.io/homebrew/brew@sha256:b0072bfdebf5934ae24b93b44a1928a88057399b3283ffa0177bb86084fdedfd";
const NIX_IMAGE: &str =
    "docker.io/nixos/nix@sha256:7a007c766426c1877758ddc5cb87a965ac131fc78c582ce0083d922d51ae945c";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(90);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
fn install_signal_handlers() {
    unsafe extern "C" fn handler(_: libc::c_int) {
        INTERRUPTED.store(true, Ordering::SeqCst);
    }
    unsafe {
        libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handler as *const () as libc::sighandler_t);
    }
}

#[cfg(not(unix))]
fn install_signal_handlers() {}

fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

fn seed_builder() -> Option<&'static str> {
    if command_version("cloud-localds").is_some() {
        Some("cloud-localds")
    } else if command_version("xorriso").is_some() {
        Some("xorriso")
    } else {
        None
    }
}

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

#[derive(Clone, Debug, Default)]
struct CloudImage {
    url: String,
    sha256: String,
    arch: String,
    release: String,
}

#[derive(Debug)]
struct ProbeOutput {
    lane: String,
    kind: &'static str,
    status: i32,
    lane_status: &'static str,
    image: Option<String>,
    manifest_digest: Option<String>,
    architecture: Option<String>,
    package_manager: Option<String>,
    package_manager_version: Option<String>,
    elapsed_ms: u128,
    stdout: String,
    stderr: String,
    error: Option<String>,
    cleanup: bool,
}

fn main() -> Result<()> {
    install_signal_handlers();
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
        "single" => {
            let mut backend = None;
            let mut evidence = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--backend" => backend = Some(args.next().context("--backend needs a value")?),
                    "--evidence" => {
                        evidence = Some(PathBuf::from(
                            args.next().context("--evidence needs a value")?,
                        ))
                    }
                    value => bail!("unknown single argument: {value}"),
                }
            }
            probe_single(
                backend.as_deref().context("single requires --backend")?,
                evidence.as_deref(),
            )
        }
        "probe-nix" => {
            let mut evidence = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--evidence" => {
                        evidence = Some(PathBuf::from(
                            args.next().context("--evidence needs a value")?,
                        ))
                    }
                    value => bail!("unknown probe-nix argument: {value}"),
                }
            }
            let probe = probe_nix_image(NIX_IMAGE);
            let json = render_probe(&probe);
            write_evidence(evidence.as_deref(), &format!("{json}\n"))?;
            println!("{json}");
            if probe.status != 0 {
                bail!("Nix lane failed with status {}", probe.status);
            }
            Ok(())
        }
        "verify-cloud-image" => verify_cloud_image_lock(),
        "audit-cleanup" => audit_cleanup(),
        "vm-run" => vm_run(),
        "vm-interrupt-test" => vm_interrupt_test(),
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
            probe_all(evidence.as_deref())
        }
        _ => bail!(
            "usage: package-manager-matrix <doctor|list-images|probe-alpine|single|probe-nix|all|vm-run|vm-interrupt-test> [options]"
        ),
    }
}

fn doctor() -> Result<()> {
    let podman = command_version("podman");
    let qemu = command_version("qemu-system-x86_64");
    let kvm = Path::new("/dev/kvm").exists();
    let cloud_localds = command_version("cloud-localds");
    let xorriso = command_version("xorriso");
    let mut missing = Vec::new();
    if podman.is_none() {
        missing.push("podman");
    }
    if qemu.is_none() {
        missing.push("qemu-system-x86_64");
    }
    if !kvm {
        missing.push("/dev/kvm");
    }
    if seed_builder().is_none() {
        missing.push("cloud-localds or xorriso");
    }
    let status = if missing.is_empty() { "pass" } else { "fail" };
    println!(
        "{{\"status\":{},\"podman\":{},\"qemu\":{},\"kvm\":{},\"cloud_localds\":{},\"xorriso\":{},\"seed_builder\":{},\"missing\":{}}}",
        json_string(status),
        json_string(podman.as_deref().unwrap_or("MISSING")),
        json_string(qemu.as_deref().unwrap_or("MISSING")),
        kvm,
        json_string(cloud_localds.as_deref().unwrap_or("MISSING")),
        json_string(xorriso.as_deref().unwrap_or("MISSING")),
        json_string(seed_builder().unwrap_or("MISSING")),
        json_array(&missing)
    );
    if !missing.is_empty() {
        bail!(
            "doctor failed: required matrix tooling is missing ({})",
            missing.join(", ")
        );
    }
    Ok(())
}

fn list_images(path: &Path) -> Result<()> {
    let images = read_lock(path)?;
    let mut failures = Vec::new();
    for image in images {
        let validation = validate_image(&image)
            .and_then(|_| verify_manifest(&image))
            .and_then(|_| {
                probe_command(&image.manager).context("locked lane has no executable probe")
            });
        let error = validation.as_ref().err().map(ToString::to_string);
        if error.is_some() {
            failures.push(image.name.clone());
        }
        println!(
            "{{\"name\":{},\"registry\":{},\"tag\":{},\"manifest_digest\":{},\"architecture\":{},\"package_manager\":{},\"version\":{},\"lane\":{},\"probe\":{},\"validation\":{},\"error\":{}}}",
            json_string(&image.name),
            json_string(&image.registry),
            json_string(&image.tag),
            json_string(&image.digest),
            json_string(&image.arch),
            json_string(&image.manager),
            json_string(&image.version),
            json_string(&image.name),
            json_string(
                probe_command(&image.manager)
                    .map(|_| "supported")
                    .unwrap_or("unavailable")
            ),
            json_string(if error.is_some() { "fail" } else { "pass" }),
            error
                .as_deref()
                .map(json_string)
                .unwrap_or_else(|| "null".into())
        );
    }
    if !failures.is_empty() {
        bail!("image lock validation failed for: {}", failures.join(", "));
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
        if body.contains("\"manifests\"") {
            match manifest_architecture(&body, &image.digest) {
                Some(architecture) if architecture == image.arch => {}
                Some(architecture) => bail!(
                    "locked digest {} resolves architecture {}, expected {}",
                    image.digest,
                    architecture,
                    image.arch
                ),
                None => bail!(
                    "manifest {} has no platform architecture for locked digest {}",
                    reference,
                    image.digest
                ),
            }
        }
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

fn manifest_architecture(body: &str, digest: &str) -> Option<String> {
    let marker = format!("\"digest\": \"{digest}\"");
    let mut matched = false;
    for line in body.lines() {
        let line = line.trim();
        if !matched {
            if line.starts_with(&marker) {
                matched = true;
            }
            continue;
        }
        let marker = "\"architecture\": \"";
        if let Some(start) = line.strip_prefix(marker) {
            let end = start.find('"')?;
            return Some(start[..end].into());
        }
    }
    None
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
    let probe = probe_image(image, ref_name, Some(full_ref), "alpine-noop");
    let json = render_probe(&probe);
    write_evidence(evidence_path, &format!("{json}\n"))?;
    println!("{json}");
    if probe.status != 0 {
        bail!("Alpine probe failed with status {}", probe.status);
    }
    Ok(())
}

fn probe_single(backend: &str, evidence_path: Option<&Path>) -> Result<()> {
    let images = read_lock(Path::new(DEFAULT_LOCK))?;
    let image = images
        .iter()
        .find(|image| image.name == backend)
        .with_context(|| format!("backend {backend} is not present in image lock"))?;
    validate_image(image).and_then(|_| verify_manifest(image))?;
    let probe = match probe_command(&image.manager) {
        Some(_) => probe_image(
            image,
            &image.digest,
            Some(format!("{}@{}", image.registry, image.digest)),
            "single-backend",
        ),
        None => unavailable_probe(
            backend,
            "container",
            None,
            format!("no executable probe for {}", image.manager),
        ),
    };
    let json = render_probe(&probe);
    write_evidence(evidence_path, &format!("{json}\n"))?;
    println!("{json}");
    if probe.status != 0 {
        bail!(
            "single backend {backend} failed with status {}",
            probe.status
        );
    }
    Ok(())
}

fn probe_all(evidence_path: Option<&Path>) -> Result<()> {
    let images = read_lock(Path::new(DEFAULT_LOCK))?;
    let mut records = Vec::with_capacity(images.len() + 3);
    for image in &images {
        let record = match validate_image(image).and_then(|_| verify_manifest(image)) {
            Ok(()) => match probe_command(&image.manager) {
                Some(_) => probe_image(
                    image,
                    &image.digest,
                    Some(format!("{}@{}", image.registry, image.digest)),
                    "matrix-container",
                ),
                None => unavailable_probe(
                    &image.name,
                    "container",
                    Some(format!("{}@{}", image.registry, image.digest)),
                    format!(
                        "locked package manager '{}' has no executable probe",
                        image.manager
                    ),
                ),
            },
            Err(error) => failed_probe(
                &image.name,
                "container",
                Some(format!("{}@{}", image.registry, image.digest)),
                error.to_string(),
            ),
        };
        records.push(record);
    }

    records.push(vm_lane_probe());
    records.push(probe_optional_image(
        "brew",
        BREW_IMAGE,
        [
            "sh", "-ec", "brew --version; printf 'LIST\\n'; brew search --formula hello; printf 'DETAILS\\n'; brew info --json=v2 hello; printf 'INSTALL\\n'; brew install hello; printf 'REMOVE\\n'; brew uninstall hello",
        ],
    ));
    records.push(probe_nix_image(NIX_IMAGE));

    let mut report = String::new();
    let mut failed = Vec::new();
    for record in &records {
        if record.status != 0 {
            failed.push(record.lane.as_str());
        }
        let json = render_probe(record);
        println!("{json}");
        report.push_str(&json);
        report.push('\n');
    }
    let aggregate_status = if failed.is_empty() { 0 } else { 1 };
    let aggregate = format!(
        "{{\"scenario\":\"matrix\",\"status\":{},\"lane_status\":{},\"lanes\":{},\"failed_lanes\":{}}}",
        aggregate_status,
        json_string(if aggregate_status == 0 {
            "pass"
        } else {
            "fail"
        }),
        records.len(),
        json_array(&failed)
    );
    println!("{aggregate}");
    report.push_str(&aggregate);
    report.push('\n');
    write_evidence(evidence_path, &report)?;
    if aggregate_status != 0 {
        bail!(
            "matrix failed or unavailable for lanes: {}",
            failed.join(", ")
        );
    }
    Ok(())
}

fn probe_image(
    image: &Image,
    digest: &str,
    full_ref: Option<String>,
    _scenario: &'static str,
) -> ProbeOutput {
    let full_ref = full_ref.unwrap_or_else(|| format!("{}@{}", image.registry, digest));
    let name = format!("pm-matrix-{}-{}", std::process::id(), unix_nanos());
    let args = probe_command(&image.manager).expect("probe_image called for supported manager");
    let mut command = Command::new("podman");
    command
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "--network",
            "bridge",
            &full_ref,
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let cleanup_guard = CleanupGuard::new(&name);
    let started = Instant::now();
    let result = run_bounded(command, COMMAND_TIMEOUT);
    drop(cleanup_guard);
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
    let package_manager_version = parse_manager_version(&image.manager, &stdout);
    let (status, error) = if status == 0 {
        match validate_manager_version(
            &image.manager,
            &image.version,
            package_manager_version.as_deref(),
        ) {
            Ok(()) => (status, error),
            Err(error) => (1, Some(error.to_string())),
        }
    } else {
        (status, error)
    };
    ProbeOutput {
        lane: image.name.clone(),
        kind: "container",
        status,
        lane_status: if status == 0 { "pass" } else { "fail" },
        image: Some(full_ref),
        manifest_digest: Some(image.digest.clone()),
        architecture: Some(image.arch.clone()),
        package_manager: Some(image.manager.clone()),
        package_manager_version,
        elapsed_ms: started.elapsed().as_millis(),
        stdout: stdout.trim().into(),
        stderr: stderr.trim().into(),
        error,
        cleanup,
    }
}

fn probe_optional_image(lane: &str, image: &str, args: [&str; 3]) -> ProbeOutput {
    let name = format!("pm-matrix-{}-{}", std::process::id(), unix_nanos());
    let mut command = Command::new("podman");
    command
        .args(["run", "--rm", "--name", &name, "--network", "bridge"])
        .arg(image)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let cleanup_guard = CleanupGuard::new(&name);
    let started = Instant::now();
    let result = run_bounded(command, COMMAND_TIMEOUT);
    drop(cleanup_guard);
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
    ProbeOutput {
        lane: lane.into(),
        kind: "optional",
        status,
        lane_status: if status == 0 { "pass" } else { "fail" },
        image: Some(image.into()),
        manifest_digest: image.split('@').nth(1).map(str::to_owned),
        architecture: Some("amd64".into()),
        package_manager: Some(lane.into()),
        package_manager_version: stdout
            .lines()
            .find(|line| !line.is_empty())
            .map(str::to_owned),
        elapsed_ms: started.elapsed().as_millis(),
        stdout: stdout.trim().into(),
        stderr: stderr.trim().into(),
        error,
        cleanup,
    }
}

const NIX_STORE_URL: &str = "local?state=/tmp/pm-matrix-nix/state&log=/tmp/pm-matrix-nix/log";

fn probe_nix_image(image: &str) -> ProbeOutput {
    let lane = "nix";
    let started = Instant::now();
    let name = format!("pm-matrix-{}-{}", std::process::id(), unix_nanos());
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(1000);
    let gid = std::process::Command::new("id")
        .arg("-g")
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(uid);
    let user = format!("{uid}:{gid}");
    let cleanup_guard = CleanupGuard::new(&name);
    let run = (|| -> Result<(String, String)> {
        let output = Command::new("podman")
            .args([
                "run",
                "--detach",
                "--rm",
                "--user",
                &user,
                "--cap-add",
                "CAP_DAC_OVERRIDE",
                "--network",
                "bridge",
                "--name",
                &name,
                "--env",
                "HOME=/tmp/pm-matrix-nix/home",
                "--env",
                "NIX_STATE_DIR=/tmp/pm-matrix-nix/state",
                "--env",
                "NIX_LOG_DIR=/tmp/pm-matrix-nix/log",
                image,
                "sh",
                "-ec",
                "mkdir -p /tmp/pm-matrix-nix/state /tmp/pm-matrix-nix/log /tmp/pm-matrix-nix/home; sleep 600",
            ])
            .output()
            .context("start non-root Nix container")?;
        if !output.status.success() {
            bail!(
                "podman run exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let exec = |script: &str| -> Result<String> {
            let output = Command::new("podman")
                .args(["exec", "--user", &user, &name, "sh", "-ec", script])
                .output()
                .with_context(|| format!("run Nix command: {script}"))?;
            if !output.status.success() {
                bail!(
                    "Nix command failed ({}): {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        };
        let flags =
            format!("--extra-experimental-features 'nix-command flakes' --store '{NIX_STORE_URL}'");
        let version = exec("nix --version")?;
        let identity = exec(
            "printf 'uid=%s\\ngid=%s\\nsudo=%s\\n' \"$(id -u)\" \"$(id -g)\" \"$(command -v sudo || true)\"",
        )?;
        if !identity
            .lines()
            .any(|line| line == format!("uid={uid}").as_str())
            || !identity
                .lines()
                .any(|line| line == format!("gid={gid}").as_str())
            || !identity.lines().any(|line| line == "sudo=")
        {
            bail!("Nix lane identity is not the current user without sudo: {identity:?}");
        }
        let before = exec(&format!("nix {flags} profile list --json"))?;
        let search = exec(&format!(
            "nix {flags} search --json nixpkgs#hello '^hello$'"
        ))?;
        let search_json: serde_json::Value =
            serde_json::from_str(search.trim()).context("parse nix search JSON")?;
        if !search_json
            .as_object()
            .is_some_and(|packages| packages.keys().any(|key| key.ends_with(".hello")))
        {
            bail!("nix search JSON did not contain hello");
        }
        exec(&format!("nix {flags} profile install nixpkgs#hello"))?;
        let after = exec(&format!("nix {flags} profile list --json"))?;
        let selector = nix_profile_element_selector(&after)?;
        exec(&format!(
            "nix {flags} profile remove {}",
            shell_quote(&selector)
        ))?;
        let removed = exec(&format!("nix {flags} profile list --json"))?;
        let removed_json: serde_json::Value =
            serde_json::from_str(removed.trim()).context("parse removed Nix profile JSON")?;
        if !removed_json
            .get("elements")
            .and_then(serde_json::Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
        {
            bail!("Nix profile still has elements after removing {selector}");
        }
        Ok((
            format!(
                "uid/gid/sudo:\n{identity}NIX_VERSION:\n{version}LIST_BEFORE:\n{}SEARCH:\n{}PROFILE_AFTER_INSTALL:\n{}SELECTOR:{selector}\nPROFILE_AFTER_REMOVE:\n{}",
                before.trim(),
                search.trim(),
                after.trim(),
                removed.trim()
            ),
            version.trim().to_owned(),
        ))
    })();
    drop(cleanup_guard);
    let cleanup = cleanup_check(&name);
    match run {
        Ok((stdout, version)) => ProbeOutput {
            lane: lane.into(),
            kind: "optional",
            status: 0,
            lane_status: "pass",
            image: Some(image.into()),
            manifest_digest: image.split('@').nth(1).map(str::to_owned),
            architecture: Some("amd64".into()),
            package_manager: Some(lane.into()),
            package_manager_version: Some(version),
            elapsed_ms: started.elapsed().as_millis(),
            stdout,
            stderr: String::new(),
            error: None,
            cleanup,
        },
        Err(error) => ProbeOutput {
            lane: lane.into(),
            kind: "optional",
            status: 1,
            lane_status: "fail",
            image: Some(image.into()),
            manifest_digest: image.split('@').nth(1).map(str::to_owned),
            architecture: Some("amd64".into()),
            package_manager: Some(lane.into()),
            package_manager_version: None,
            elapsed_ms: started.elapsed().as_millis(),
            stdout: String::new(),
            stderr: String::new(),
            error: Some(error.to_string()),
            cleanup,
        },
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn nix_profile_element_selector(json: &str) -> Result<String> {
    let document: serde_json::Value =
        serde_json::from_str(json).context("parse nix profile JSON")?;
    let elements = document
        .get("elements")
        .and_then(serde_json::Value::as_object)
        .context("Nix profile JSON missing object 'elements'")?;
    if elements.is_empty() {
        bail!("Nix profile has no active element");
    }
    elements
        .iter()
        .find_map(|(selector, element)| {
            let active = element
                .get("active")
                .and_then(serde_json::Value::as_bool)
                .is_some_and(|active| active);
            let hello = element
                .get("originalUrl")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|url| url.contains("hello"))
                || element
                    .get("attrPath")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|path| path.ends_with(".hello"));
            if active && hello && !selector.is_empty() && selector.parse::<usize>().is_err() {
                Some(selector.clone())
            } else {
                None
            }
        })
        .context("Nix profile JSON has no stable active hello element selector")
}

fn failed_probe(
    lane: &str,
    kind: &'static str,
    image: Option<String>,
    error: String,
) -> ProbeOutput {
    ProbeOutput {
        lane: lane.into(),
        kind,
        status: 1,
        lane_status: "fail",
        image,
        manifest_digest: None,
        architecture: None,
        package_manager: None,
        package_manager_version: None,
        elapsed_ms: 0,
        stdout: String::new(),
        stderr: String::new(),
        error: Some(error),
        cleanup: true,
    }
}

fn unavailable_probe(
    lane: &str,
    kind: &'static str,
    image: Option<String>,
    reason: impl Into<String>,
) -> ProbeOutput {
    ProbeOutput {
        lane: lane.into(),
        kind,
        status: 1,
        lane_status: "unavailable",
        image,
        manifest_digest: None,
        architecture: None,
        package_manager: None,
        package_manager_version: None,
        elapsed_ms: 0,
        stdout: String::new(),
        stderr: String::new(),
        error: Some(reason.into()),
        cleanup: true,
    }
}

fn vm_lane_probe() -> ProbeOutput {
    match read_cloud_image_lock(Path::new(DEFAULT_VM_LOCK)) {
        Ok(image) => {
            let mut missing = Vec::new();
            if command_version("qemu-system-x86_64").is_none() {
                missing.push("qemu-system-x86_64");
            }
            if !Path::new("/dev/kvm").exists() {
                missing.push("/dev/kvm");
            }
            if seed_builder().is_none() {
                missing.push("cloud-localds or xorriso");
            }
            let checksum = if missing.is_empty() {
                verify_cloud_image(&image).err().map(|e| e.to_string())
            } else {
                None
            };
            let reason = if let Some(error) = checksum {
                format!("Snap VM cloud image validation failed: {error}")
            } else if missing.is_empty() {
                format!(
                    "Snap VM orchestration is not implemented; locked cloud image {} is metadata-only",
                    image.url
                )
            } else {
                format!(
                    "Snap VM lane unavailable: missing {} (locked image {} sha256:{})",
                    missing.join(", "),
                    image.url,
                    image.sha256
                )
            };
            unavailable_probe("snap-vm", "vm", Some(image.url), reason)
        }
        Err(error) => failed_probe("snap-vm", "vm", None, error.to_string()),
    }
}

fn verify_cloud_image(image: &CloudImage) -> Result<()> {
    let path = std::env::temp_dir().join(format!("pm-matrix-cloud-{}.img", unix_nanos()));
    let result = download_cloud_image(image, &path);
    remove_cloud_temp(&path)?;
    result
}

fn remove_cloud_temp(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("remove cloud image temp file"),
    }
}

fn download_cloud_image(image: &CloudImage, path: &Path) -> Result<()> {
    (|| {
        let mut child = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(path)
            .arg(&image.url)
            .spawn()
            .context("download cloud image")?;
        let start = Instant::now();
        let status = loop {
            if interrupted() {
                terminate(&mut child);
                bail!("cloud image download interrupted");
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() >= COMMAND_TIMEOUT {
                terminate(&mut child);
                bail!(
                    "cloud image download exceeded {}s",
                    COMMAND_TIMEOUT.as_secs()
                );
            }
            thread::sleep(Duration::from_millis(25));
        };
        if !status.success() {
            bail!("curl exited with {status}");
        }
        let output = Command::new("sha256sum")
            .arg(path)
            .output()
            .context("hash cloud image")?;
        if !output.status.success() {
            bail!("sha256sum exited with {}", output.status);
        }
        let hash_output = String::from_utf8_lossy(&output.stdout);
        let actual = hash_output.split_whitespace().next().unwrap_or("");
        if actual != image.sha256 {
            bail!(
                "cloud image sha256 mismatch: expected {}, got {actual}",
                image.sha256
            );
        }
        Ok(())
    })()
}

fn verify_cloud_image_lock() -> Result<()> {
    let image = read_cloud_image_lock(Path::new(DEFAULT_VM_LOCK))?;
    verify_cloud_image(&image)?;
    println!(
        "{{\"status\":\"pass\",\"url\":{},\"sha256\":{},\"cleanup\":true}}",
        json_string(&image.url),
        json_string(&image.sha256)
    );
    Ok(())
}

fn vm_run() -> Result<()> {
    let image = read_cloud_image_lock(Path::new(DEFAULT_VM_LOCK))?;
    let missing = ["qemu-system-x86_64", "qemu-img"]
        .into_iter()
        .filter(|tool| command_version(tool).is_none())
        .collect::<Vec<_>>();
    if seed_builder().is_none() {
        let mut missing = missing;
        missing.push("cloud-localds or xorriso");
        return vm_unavailable(&image, &missing);
    }
    if !Path::new("/dev/kvm").exists() {
        let mut missing = missing;
        missing.push("/dev/kvm");
        return vm_unavailable(&image, &missing);
    }
    if !missing.is_empty() {
        return vm_unavailable(&image, &missing);
    }
    let work = std::env::temp_dir().join(format!("pm-matrix-vm-{}", unix_nanos()));
    fs::create_dir_all(&work)?;
    let mut cleanup = VmCleanup::new(work.clone());
    let base = work.join("ubuntu.img");
    download_cloud_image(&image, &base)?;
    let overlay = work.join("overlay.qcow2");
    let backing_format = qemu_image_format(&base)?;
    let status = Command::new("qemu-img")
        .args(["create", "-f", "qcow2", "-F"])
        .arg(&backing_format)
        .arg("-b")
        .arg(&base)
        .arg(&overlay)
        .status()
        .context("create VM overlay")?;
    if !status.success() {
        bail!("qemu-img exited with {status}");
    }
    let user_data = work.join("user-data");
    let meta_data = work.join("meta-data");
    let seed = work.join("seed.iso");
    fs::write(&user_data, b"#cloud-config\nruncmd:\n  - [ sh, -c, 'echo PM_MATRIX_GUEST_OK > /dev/ttyS0; poweroff -f' ]\n")?;
    fs::write(
        &meta_data,
        b"instance-id: pm-matrix\nlocal-hostname: pm-matrix\n",
    )?;
    let seed_tool = seed_builder().context("no cloud-init seed builder")?;
    let status = if seed_tool == "cloud-localds" {
        Command::new(seed_tool)
            .arg(&seed)
            .arg(&user_data)
            .arg(&meta_data)
            .status()
    } else {
        Command::new(seed_tool)
            .args([
                "-as", "mkisofs", "-volid", "cidata", "-joliet", "-rock", "-o",
            ])
            .arg(&seed)
            .arg(&user_data)
            .arg(&meta_data)
            .status()
    }
    .context("create cloud-init seed")?;
    if !status.success() {
        bail!("cloud-localds exited with {status}");
    }
    let child = Command::new("qemu-system-x86_64")
        .args([
            "-enable-kvm",
            "-nographic",
            "-serial",
            "mon:stdio",
            "-m",
            "1024",
            "-drive",
        ])
        .arg(format!("file={},if=virtio,format=qcow2", overlay.display()))
        .args([
            "-drive",
            &format!("file={},if=virtio,format=raw", seed.display()),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn QEMU")?;
    cleanup.child = Some(child);
    let result = run_child_timeout(cleanup.child.as_mut().unwrap(), Duration::from_secs(120))?;
    cleanup.child = None;
    if !result.stdout.contains("PM_MATRIX_GUEST_OK") {
        bail!("guest probe did not report PM_MATRIX_GUEST_OK");
    }
    Ok(())
}

fn qemu_image_format(path: &Path) -> Result<String> {
    let output = Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(path)
        .output()
        .context("inspect cloud image format")?;
    if !output.status.success() {
        bail!("qemu-img info exited with {}", output.status);
    }
    let info: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse qemu-img format metadata")?;
    let format = info
        .get("format")
        .and_then(serde_json::Value::as_str)
        .filter(|format| !format.is_empty())
        .context("qemu-img metadata has no format")?;
    Ok(format.to_string())
}

fn vm_unavailable(image: &CloudImage, missing: &[&str]) -> Result<()> {
    println!(
        "{{\"scenario\":\"snap-vm\",\"status\":1,\"lane_status\":\"unavailable\",\"missing\":{},\"image\":{},\"sha256\":{},\"cleanup\":true}}",
        json_array(missing),
        json_string(&image.url),
        json_string(&image.sha256)
    );
    bail!("Snap VM runner unavailable: missing {}", missing.join(", "))
}

fn vm_interrupt_test() -> Result<()> {
    let work = std::env::temp_dir().join(format!("pm-matrix-vm-interrupt-{}", unix_nanos()));
    fs::create_dir_all(&work)?;
    let marker = work.join("overlay.qcow2");
    fs::write(&marker, b"interruption fixture")?;
    let mut cleanup = VmCleanup::new(work.clone());
    cleanup.child = Some(Command::new("sleep").arg("60").spawn()?);
    let timed_out =
        run_child_timeout(cleanup.child.as_mut().unwrap(), Duration::from_millis(100)).is_err();
    cleanup.child = None;
    drop(cleanup);
    let clean = !work.exists();
    println!(
        "{{\"scenario\":\"snap-vm-interruption\",\"timed_out\":{},\"cleanup\":{},\"workdir\":{}}}",
        timed_out,
        clean,
        json_string(&work.display().to_string())
    );
    if !timed_out || !clean {
        bail!("interruption cleanup failed");
    }
    Ok(())
}

fn run_child_timeout(child: &mut Child, timeout: Duration) -> Result<Output> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    pipe.read_to_string(&mut stdout)?;
                }
                if let Some(mut pipe) = child.stderr.take() {
                    pipe.read_to_string(&mut stderr)?;
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            bail!("child exited with {status}");
        }
        if interrupted() {
            terminate(child);
            let _output = child_output(child)?;
            bail!("child interrupted");
        }
        if start.elapsed() >= timeout {
            terminate(child);
            let output = child_output(child)?;
            let serial = output.stdout.trim();
            if serial.is_empty() {
                bail!(
                    "VM exceeded {}ms timeout (guest serial empty)",
                    timeout.as_millis()
                );
            }
            bail!(
                "VM exceeded {}ms timeout (guest serial: {})",
                timeout.as_millis(),
                serial
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn child_output(child: &mut Child) -> Result<Output> {
    let status = child.wait().context("wait for terminated child")?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

struct VmCleanup {
    work: PathBuf,
    child: Option<Child>,
}

impl VmCleanup {
    fn new(work: PathBuf) -> Self {
        Self { work, child: None }
    }
}

impl Drop for VmCleanup {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            terminate(child);
        }
        let _ = fs::remove_dir_all(&self.work);
    }
}

fn audit_cleanup() -> Result<()> {
    let containers = Command::new("podman")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .context("inspect podman containers")?;
    if !containers.status.success() {
        bail!(
            "podman cleanup inspection exited with {}: {}",
            containers.status,
            String::from_utf8_lossy(&containers.stderr).trim()
        );
    }
    let leftovers: Vec<_> = String::from_utf8_lossy(&containers.stdout)
        .lines()
        .filter(|line| line.starts_with("pm-matrix-"))
        .map(str::to_owned)
        .collect();
    let qemu_status = Command::new("pgrep")
        .args(["-f", "qemu-system-x86_64.*pm-matrix"])
        .status()
        .context("inspect qemu processes")?;
    let qemu = match qemu_status.code() {
        Some(1) => false,
        Some(0) => true,
        _ => bail!("pgrep qemu inspection exited with {qemu_status}"),
    };
    let temp_leftovers = fs::read_dir(std::env::temp_dir())?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|name| name.starts_with("pm-matrix-") || name.contains("qcow2"))
        .collect::<Vec<_>>();
    let networks = Command::new("podman")
        .args(["network", "ls", "--format", "{{.Name}}"])
        .output()
        .context("inspect podman networks")?;
    if !networks.status.success() {
        bail!("podman network inspection exited with {}", networks.status);
    }
    let network_leftovers = String::from_utf8_lossy(&networks.stdout)
        .lines()
        .filter(|line| line.starts_with("pm-matrix-"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let clean =
        leftovers.is_empty() && !qemu && temp_leftovers.is_empty() && network_leftovers.is_empty();
    println!(
        "{{\"status\":{},\"containers\":{},\"qemu\":{},\"temp\":{},\"cleanup\":{}}}",
        json_string(if clean { "pass" } else { "fail" }),
        json_array(&leftovers.iter().map(String::as_str).collect::<Vec<_>>()),
        qemu,
        json_array(
            &temp_leftovers
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        ),
        clean
    );
    if !clean {
        bail!("matrix resources remain after interrupted run");
    }
    Ok(())
}

fn render_probe(probe: &ProbeOutput) -> String {
    let scenario = if probe.kind == "container" {
        if probe.lane == "alpine" {
            "alpine-noop"
        } else {
            "container-probe"
        }
    } else {
        "matrix-lane"
    };
    format!(
        "{{\"scenario\":{},\"lane\":{},\"kind\":{},\"status\":{},\"lane_status\":{},\"image\":{},\"manifest_digest\":{},\"architecture\":{},\"package_manager\":{},\"package_manager_version\":{},\"elapsed_ms\":{},\"stdout\":{},\"stderr\":{},\"error\":{},\"cleanup\":{}}}",
        json_string(scenario),
        json_string(&probe.lane),
        json_string(probe.kind),
        probe.status,
        json_string(probe.lane_status),
        probe
            .image
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        probe
            .manifest_digest
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        probe
            .architecture
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        probe
            .package_manager
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        probe
            .package_manager_version
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        probe.elapsed_ms,
        json_string(&probe.stdout),
        json_string(&probe.stderr),
        probe
            .error
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
        probe.cleanup
    )
}

fn write_evidence(path: Option<&Path>, content: &str) -> Result<()> {
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
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

struct CleanupGuard {
    name: String,
}

impl CleanupGuard {
    fn new(name: &str) -> Self {
        Self { name: name.into() }
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = Command::new("podman")
            .args(["rm", "--force", "--ignore", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn probe_command(manager: &str) -> Option<&'static [&'static str]> {
    match manager {
        "apk" => Some(&[
            "sh",
            "-ec",
            "apk --version; printf 'LIST\\n'; apk search busybox; printf 'DETAILS\\n'; apk info busybox; printf 'INSTALL\\n'; apk add --no-cache curl; printf 'REMOVE\\n'; apk del curl",
        ]),
        "apt" => Some(&[
            "sh",
            "-ec",
            "apt-get --version; apt-get update -qq; printf 'LIST\\n'; apt-cache search '^hello$'; printf 'DETAILS\\n'; apt-cache show hello; printf 'INSTALL\\n'; apt-get install -y --no-install-recommends hello; printf 'REMOVE\\n'; apt-get remove -y hello",
        ]),
        "dnf5" => Some(&[
            "sh",
            "-ec",
            "dnf5 --version; printf 'LIST\\n'; dnf5 list installed bash; printf 'DETAILS\\n'; dnf5 info bash; printf 'INSTALL\\n'; dnf5 install -y --setopt=install_weak_deps=False hello; printf 'REMOVE\\n'; dnf5 remove -y hello",
        ]),
        "dnf4" => Some(&[
            "sh",
            "-ec",
            "dnf --version; printf 'LIST\\n'; dnf list installed bash; printf 'DETAILS\\n'; dnf info bash; printf 'INSTALL\\n'; dnf install -y --setopt=install_weak_deps=False tree; printf 'REMOVE\\n'; dnf remove -y tree",
        ]),
        "zypper" => Some(&[
            "sh",
            "-ec",
            "zypper --version; printf 'LIST\\n'; zypper --non-interactive search bash; printf 'DETAILS\\n'; zypper --non-interactive info bash; printf 'INSTALL\\n'; zypper --non-interactive --no-gpg-checks install hello; printf 'REMOVE\\n'; zypper --non-interactive remove hello",
        ]),
        "xbps" => Some(&[
            "sh",
            "-ec",
            "xbps-query --version; xbps-install -i -S -R https://repo-default.voidlinux.org/current/musl; xbps-install -i -y -R https://repo-default.voidlinux.org/current/musl -u xbps; printf 'LIST\\n'; xbps-query -l; printf 'DETAILS\\n'; xbps-query -S xbps; printf 'INSTALL\\n'; xbps-install -i -y -R https://repo-default.voidlinux.org/current/musl curl; printf 'REMOVE\\n'; xbps-remove -Ry curl",
        ]),
        _ => None,
    }
}

fn parse_manager_version(manager: &str, stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?.trim();
    let version = match manager {
        "dnf5" => line.strip_prefix("dnf5 version "),
        "dnf4" => line.split_whitespace().next(),
        "zypper" => line.strip_prefix("zypper "),
        "xbps" => line
            .strip_prefix("XBPS: ")
            .and_then(|v| v.split_whitespace().next()),
        "apk" => line
            .strip_prefix("apk-tools ")
            .and_then(|v| v.split(',').next()),
        "apt" => line
            .strip_prefix("apt ")
            .and_then(|v| v.split_whitespace().next()),
        _ => None,
    }?;
    Some(version.to_string())
}

fn validate_manager_version(manager: &str, expected: &str, observed: Option<&str>) -> Result<()> {
    if observed != Some(expected) {
        bail!(
            "{} version mismatch: expected {}, observed {}",
            manager,
            expected,
            observed.unwrap_or("unavailable")
        );
    }
    Ok(())
}

fn validate_image(image: &Image) -> Result<()> {
    if image.name.is_empty()
        || image.registry.is_empty()
        || image.tag.is_empty()
        || image.arch.is_empty()
        || image.manager.is_empty()
        || image.version.is_empty()
    {
        bail!("locked image {} has missing required fields", image.name);
    }
    validate_digest(&image.digest)
}

fn read_cloud_image_lock(path: &Path) -> Result<CloudImage> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read cloud image lock {}", path.display()))?;
    let mut image = CloudImage::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("invalid cloud image lock line: {line}"))?;
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "url" => image.url = value.into(),
            "sha256" => image.sha256 = value.into(),
            "architecture" => image.arch = value.into(),
            "release" => image.release = value.into(),
            _ => {}
        }
    }
    if !image.url.starts_with("https://") {
        bail!("cloud image lock URL must use https");
    }
    if image.sha256.len() != 64 || !image.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("cloud image lock has invalid sha256");
    }
    if image.arch.is_empty() || image.release.is_empty() {
        bail!("cloud image lock is missing architecture or release");
    }
    Ok(image)
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

fn json_array(values: &[&str]) -> String {
    let mut output = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push_str(&json_string(value));
    }
    output.push(']');
    output
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
        assert!(validate_digest(&format!("sha256:{}", "g".repeat(64))).is_err());
        assert!(validate_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn nonzero_guest_exit_is_reported_as_failure() {
        let mut child = Command::new("sh").args(["-c", "exit 23"]).spawn().unwrap();
        let error = match run_child_timeout(&mut child, Duration::from_secs(2)) {
            Ok(_) => panic!("nonzero guest exit was accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("exit status: 23"));
    }

    #[test]
    fn cleanup_error_is_returned_with_context() {
        let directory = std::env::temp_dir().join(format!("pm-matrix-cleanup-{}", unix_nanos()));
        fs::create_dir(&directory).unwrap();
        let error = remove_cloud_temp(&directory).unwrap_err();
        assert!(error.to_string().contains("remove cloud image temp file"));
        fs::remove_dir(&directory).unwrap();
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
    fn every_locked_container_has_a_probe_command() {
        let images = read_lock(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/package-managers/images.lock")
                .as_path(),
        )
        .unwrap();
        assert_eq!(images.len(), 8);
        let mut names = images
            .iter()
            .map(|image| image.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), images.len());
        for image in &images {
            validate_image(image).unwrap();
            assert!(probe_command(&image.manager).is_some(), "{}", image.name);
        }
    }
    #[test]
    fn cloud_image_lock_is_metadata_and_validated() {
        let image = read_cloud_image_lock(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/package-managers/ubuntu-cloud-image.lock")
                .as_path(),
        )
        .unwrap();
        assert!(image.url.starts_with("https://"));
        assert_eq!(image.sha256.len(), 64);
        assert!(!image.release.is_empty());
    }
    #[test]
    fn unavailable_lane_is_never_rendered_as_a_pass() {
        let probe = unavailable_probe("nix", "optional", None, "not locked");
        let rendered = render_probe(&probe);
        assert_eq!(probe.status, 1);
        assert!(rendered.contains("\"lane_status\":\"unavailable\""));
        assert!(rendered.contains("\"status\":1"));
    }
    #[test]
    fn manifest_architecture_is_bound_to_the_locked_digest() {
        let body = r#"
            "digest": "sha256:wrong",
            "architecture": "arm64",
            "digest": "sha256:right",
            "architecture": "amd64",
        "#;
        assert_eq!(
            manifest_architecture(body, "sha256:right").as_deref(),
            Some("amd64")
        );
    }
    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(json_string("line\nq\t\u{0001}"), "\"line\\nq\\t\\u0001\"");
        assert!(!json_string("q\n").contains('\n'));
    }
    #[test]
    fn probe_version_requires_exact_observed_value() {
        let observed =
            parse_manager_version("apk", "apk-tools 2.14.4, compiled for x86_64.\n").unwrap();
        assert!(validate_manager_version("apk", "2.14.4-r0", Some(&observed)).is_err());
        assert!(validate_manager_version("apk", "2.14.4", Some(&observed)).is_ok());
    }

    #[test]
    fn nix_profile_selector_uses_stable_element_id() {
        let json = r#"{"version":3,"elements":{"nixpkgs#hello":{"active":true,"originalUrl":"nixpkgs#hello","storePaths":["/tmp/nix/store/hello"]}}}"#;
        assert_eq!(nix_profile_element_selector(json).unwrap(), "nixpkgs#hello");
    }

    #[test]
    fn nix_profile_selector_rejects_missing_or_index_only_schema() {
        assert!(nix_profile_element_selector(r#"{"version":3}"#).is_err());
        assert!(
            nix_profile_element_selector(
                r#"{"version":3,"elements":{"0":{"active":true,"originalUrl":"nixpkgs#hello"}}}"#
            )
            .is_err()
        );
        assert!(nix_profile_element_selector(
            r#"{"version":3,"elements":{"nixpkgs#hello":{"active":false,"originalUrl":"nixpkgs#hello"}}}"#
        )
        .is_err());
        assert!(
            nix_profile_element_selector(
                r#"{"version":3,"elements":{"nixpkgs#hello":{"originalUrl":"nixpkgs#hello"}}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn nix_profile_selector_reports_invalid_json() {
        let error = nix_profile_element_selector("not-json").unwrap_err();
        assert!(error.to_string().contains("parse nix profile JSON"));
    }
}
