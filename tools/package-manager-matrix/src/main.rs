use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_LOCK: &str = "tests/package-managers/images.lock";
const DEFAULT_VM_LOCK: &str = "tests/package-managers/ubuntu-cloud-image.lock";
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
            "usage: package-manager-matrix <doctor|list-images|probe-alpine|single|all|vm-run|vm-interrupt-test> [options]"
        ),
    }
}

fn doctor() -> Result<()> {
    let podman = command_version("podman");
    let qemu = command_version("qemu-system-x86_64");
    let kvm = Path::new("/dev/kvm").exists();
    let cloud_init = command_version("cloud-init");
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
    if cloud_init.is_none() {
        missing.push("cloud-init");
    }
    let status = if missing.is_empty() { "pass" } else { "fail" };
    println!(
        "{{\"status\":{},\"podman\":{},\"qemu\":{},\"kvm\":{},\"cloud_init\":{},\"cloud_init_required\":true,\"missing\":{}}}",
        json_string(status),
        json_string(podman.as_deref().unwrap_or("MISSING")),
        json_string(qemu.as_deref().unwrap_or("MISSING")),
        kvm,
        json_string(cloud_init.as_deref().unwrap_or("MISSING")),
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
    records.push(unavailable_probe(
        "brew",
        "optional",
        None,
        "Linuxbrew lane has no locked executable image or runner",
    ));
    records.push(unavailable_probe(
        "nix",
        "optional",
        None,
        "Nix profile lane has no locked executable image or runner",
    ));

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
            "none",
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
    let package_manager_version =
        parse_manager_version(&image.manager, &stdout).or_else(|| Some(image.version.clone()));
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
            if command_version("cloud-init").is_none() {
                missing.push("cloud-init");
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
    let _ = fs::remove_file(&path);
    result
}

fn download_cloud_image(image: &CloudImage, path: &Path) -> Result<()> {
    let result = (|| {
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(path)
            .arg(&image.url)
            .status()
            .context("download cloud image")?;
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
    })();
    result
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
    let missing = ["qemu-system-x86_64", "qemu-img", "cloud-init"]
        .into_iter()
        .filter(|tool| command_version(tool).is_none())
        .collect::<Vec<_>>();
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
    let status = Command::new("qemu-img")
        .args(["create", "-f", "qcow2", "-F", "raw", "-b"])
        .arg(&base)
        .arg(&overlay)
        .status()
        .context("create VM overlay")?;
    if !status.success() {
        bail!("qemu-img exited with {status}");
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn QEMU")?;
    cleanup.child = Some(child);
    let result = run_child_timeout(cleanup.child.as_mut().unwrap(), Duration::from_secs(30));
    cleanup.child = None;
    result
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

fn run_child_timeout(child: &mut Child, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            terminate(child);
            bail!("VM exceeded {}ms timeout", timeout.as_millis());
        }
        thread::sleep(Duration::from_millis(25));
    }
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
    let leftovers: Vec<_> = String::from_utf8_lossy(&containers.stdout)
        .lines()
        .filter(|line| line.starts_with("pm-matrix-"))
        .map(str::to_owned)
        .collect();
    let qemu = Command::new("pgrep")
        .args(["-f", "qemu-system-x86_64.*pm-matrix"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    let temp_leftovers = fs::read_dir(std::env::temp_dir())
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| name.starts_with("pm-matrix-") || name.contains("qcow2"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let clean = leftovers.is_empty() && !qemu && temp_leftovers.is_empty();
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
        "apk" => Some(&["apk", "--print-arch"]),
        "apt" => Some(&["apt-get", "--version"]),
        "dnf5" => Some(&["dnf5", "--version"]),
        "dnf4" => Some(&["dnf", "--version"]),
        "zypper" => Some(&["zypper", "--version"]),
        "xbps" => Some(&["xbps-query", "--version"]),
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
    fn every_locked_container_has_a_probe_command() {
        let images = read_lock(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/package-managers/images.lock")
                .as_path(),
        )
        .unwrap();
        assert!(images.len() >= 8);
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
}
