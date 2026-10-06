use super::query_reload_bind;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new(provider: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "packtide-query-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("state"), "0\n").unwrap();
        let path = root.join("provider");
        fs::write(&path, provider).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        Self(root)
    }

    fn command(&self, query: &str, refresh: bool) -> Command {
        let bind = query_reload_bind(&self.0.join("provider"), refresh);
        let command = bind
            .strip_prefix("change:reload(")
            .or_else(|| bind.strip_prefix("change:reload-sync("))
            .unwrap()
            .strip_suffix(')')
            .unwrap();
        let command = command.replace("{q}", &crate::ui::shell_quote(query));
        let mut shell = Command::new("sh");
        shell
            .args(["-c", &command])
            .process_group(0)
            .env("PACKTIDE_QUERY_STATE", self.0.join("state"));
        shell.env("QUERY_FIXTURE", &self.0);
        shell
    }

    fn run(&self, query: &str, refresh: bool) -> Output {
        self.command(query, refresh).output().unwrap()
    }

    fn wait_for_file(&self, name: &str) {
        let started = Instant::now();
        while !self.0.join(name).exists() {
            assert!(started.elapsed() < Duration::from_secs(5), "missing {name}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn query_reload_provider_stays_in_fzf_cancellation_group() {
    // Given: fzf owns the reload shell's process group and kills that group.
    let fixture = Fixture::new("#!/bin/sh\nps -o pgid= -p $$\n");
    // When: the generated reload command starts its provider.
    let child = fixture
        .command("hello", false)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let group = child.id();
    let output = child.wait_with_output().unwrap();
    // Then: the provider cannot survive fzf killing the reload process group.
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        group.to_string()
    );
}

#[test]
fn query_reload_does_not_wait_for_a_lock_left_by_forceful_cancellation() {
    // Given: an earlier reload was killed while holding the generation lock.
    let fixture = Fixture::new("#!/bin/sh\nprintf 'fresh rows\\n'\n");
    fs::create_dir(fixture.0.join("state.lock")).unwrap();
    let mut child = fixture
        .command("hello", false)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // When: a newer query is submitted after the forceful cancellation.
    let deadline = Instant::now() + Duration::from_secs(2);
    let finished = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if Instant::now() >= deadline {
            Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status()
                .unwrap();
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    // Then: stale lock state cannot block the next query.
    assert!(finished, "reload waited for an abandoned generation lock");
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"fresh rows\n");
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn query_reload_propagates_provider_failure_without_partial_rows() {
    // Given: a provider that fails after printing incomplete output.
    let fixture = Fixture::new("#!/bin/sh\nprintf 'partial rows\\n'\nexit 37\n");
    // When: fzf runs the generated reload command.
    let output = fixture.run("hello", false);
    // Then: incomplete rows are withheld and failure is not reported as success.
    assert_eq!(output.status.code(), Some(37));
    assert!(output.stdout.is_empty());
}

#[test]
fn query_reload_trims_and_preserves_query_as_one_argument() {
    // Given: a provider that prints each argument separately.
    let fixture = Fixture::new("#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n");
    // When: a query containing shell syntax is submitted.
    let output = fixture.run("  hello '$(false); world  ", false);
    // Then: the trimmed query remains one literal argument.
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"<install>\n<hello '$(false); world>\n");
}

#[test]
fn query_reload_refresh_keeps_active_query() {
    // Given: a provider that prints each argument separately.
    let fixture = Fixture::new("#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n");
    // When: the user refreshes while a query is active.
    let output = fixture.run("hello", true);
    // Then: refresh does not silently revert to the blank catalog.
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"<install>\n<--refresh>\n<hello>\n");
}

#[test]
fn query_reload_restores_catalog_when_query_is_cleared_or_short() {
    // Given: a provider that prints each argument separately.
    let fixture = Fixture::new("#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n");
    // When: the query is empty, whitespace-only, ASCII-short or Unicode-short.
    let outputs = ["", "  ", "h", "好"].map(|query| fixture.run(query, false));
    // Then: query-required providers are not called with any short query.
    for output in outputs {
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"<install>\n");
    }
}

#[test]
fn query_reload_cancels_running_stale_provider_and_suppresses_its_rows() {
    // Given: an old provider blocks after printing a partial result.
    let fixture = Fixture::new(
        "#!/bin/sh\nif [ \"$2\" = old ]; then\ntrap 'touch \"$QUERY_FIXTURE/cancelled\"; exit 0' TERM\nprintf 'old rows\\n'\ntouch \"$QUERY_FIXTURE/started\"\nsleep 10 &\nwait\nelse\nprintf 'new rows\\n'\nfi\n",
    );
    let old = fixture
        .command("old", false)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait_for_file("started");
    // When: a newer query supersedes the running provider.
    let new_output = fixture.run("new", false);
    let old_output = old.wait_with_output().unwrap();
    // Then: the process group is cancelled and only fresh rows are published.
    assert!(new_output.status.success(), "{:?}", new_output);
    assert_eq!(new_output.stdout, b"new rows\n");
    assert!(old_output.status.success(), "{:?}", old_output);
    assert!(old_output.stdout.is_empty());
    fixture.wait_for_file("cancelled");
}

#[test]
fn query_reload_debounces_superseded_queries_before_provider_spawn() {
    // Given: the debounce interval is held at a deterministic rendezvous.
    let fixture = Fixture::new("#!/bin/sh\nprintf '%s\\n' \"$2\"\n");
    let sleep = fixture.0.join("sleep");
    fs::write(
        &sleep,
        "#!/bin/sh\nif [ \"$1\" = 0.3 ]; then\ntouch \"$QUERY_FIXTURE/debounce-$PPID\"\nwhile [ ! -f \"$QUERY_FIXTURE/release\" ]; do /bin/sleep 0.01; done\nelse\nexec /bin/sleep \"$@\"\nfi\n",
    )
    .unwrap();
    fs::set_permissions(sleep, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", fixture.0.display(), std::env::var("PATH").unwrap());
    let spawn = |query| {
        fixture
            .command(query, false)
            .env("PATH", &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let old = spawn("old");
    let started = Instant::now();
    while fs::read_to_string(fixture.0.join("state")).unwrap().trim() != old.id().to_string() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let new = spawn("new");
    while fs::read_dir(&fixture.0)
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("debounce-"))
        .count()
        < 2
    {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    // When: both query generations finish their debounce interval.
    fs::write(fixture.0.join("release"), "").unwrap();
    let outputs = [old, new].map(|child| child.wait_with_output().unwrap());
    // Then: only the newest generation invokes and publishes provider output.
    assert!(outputs.iter().all(|output| output.status.success()));
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.stdout.is_empty())
            .count(),
        1
    );
    let current = outputs
        .iter()
        .find(|output| !output.stdout.is_empty())
        .unwrap();
    assert_eq!(current.stdout, b"new\n");
}
