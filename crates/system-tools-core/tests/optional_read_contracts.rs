use std::fs;
use std::os::unix::fs::PermissionsExt;
use system_tools_core::{BackendError, BackendId, BuiltinBackend, CapabilitySet, PackageBackend};

#[test]
fn optional_read_contracts_execute_real_command_boundaries() {
    if let Ok(scenario) = std::env::var("OPTIONAL_READ_SCENARIO") {
        let nix = BuiltinBackend::new(BackendId::Nix);
        let brew = BuiltinBackend::new(BackendId::Brew);
        match scenario.as_str() {
            "updates" => {
                assert!(nix.capabilities().contains(CapabilitySet::UPDATES));
                let result = nix.updates().unwrap();
                assert_eq!(result.packages.len(), 1);
                assert_eq!(result.packages[0].native_key.as_str(), "hello-profile");
                assert_eq!(result.packages[0].origin.as_deref(), Some("flake:nixpkgs"));
                println!("updates={:?}", result.packages);
            }
            "unchanged" => assert!(nix.updates().unwrap().packages.is_empty()),
            "malformed" | "failed" => {
                let error = nix.updates().unwrap_err();
                assert!(matches!(&error, BackendError::CommandFailed { message, .. }
                    if message.contains(if scenario == "failed" { "candidate failed" } else { "expected a string" })));
                println!("{scenario}: {error}");
            }
            "brew" => {
                let catalog = brew.catalog().unwrap();
                assert_eq!(catalog.packages.len(), 2);
                assert_eq!(catalog.packages[0].native_key.as_str(), "hello");
                let search = brew.search("/hel.*/; literal").unwrap();
                assert_eq!(search.packages.len(), 1);
                assert_eq!(search.packages[0].native_key.as_str(), "hello");
                let installed = brew.installed().unwrap();
                assert_eq!(installed.packages.len(), 1);
                assert_eq!(installed.packages[0].native_key.as_str(), "hello");
            }
            _ => panic!("unknown scenario"),
        }
        return;
    }

    // Given: strict executable fixtures accept only the real provider argv contracts.
    let directory = std::env::temp_dir().join(format!("optional-read-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let nix_body = r#"#!/bin/sh
printf 'nix' >> "$OPTIONAL_READ_LOG"; printf ' <%s>' "$@" >> "$OPTIONAL_READ_LOG"; printf '\n' >> "$OPTIONAL_READ_LOG"
if [ "$1" != '--extra-experimental-features' ] || [ "$2" != 'nix-command flakes' ]; then exit 91; fi
shift 2
case "$1" in
profile)
  [ "$#" = 3 ] && [ "$2" = list ] && [ "$3" = --json ] || exit 92
  printf '%s\n' '{"elements":{"hello-profile":{"attrPath":"legacyPackages.x86_64-linux.hello","originalUrl":"flake:nixpkgs","storePaths":["/nix/store/old-hello"],"active":true},"inactive":{"attrPath":"hello","active":false}}}'
  ;;
eval)
  [ "$#" = 4 ] && [ "$2" = --json ] && [ "$3" = --refresh ] && [ "$4" = 'flake:nixpkgs#legacyPackages.x86_64-linux.hello.outPath' ] || exit 93
  case "$OPTIONAL_READ_SCENARIO" in
    unchanged) printf '%s\n' '"/nix/store/old-hello"';;
    malformed) printf '%s\n' '{"outPath":"not-a-string"}';;
    failed) printf 'candidate failed\n' >&2; exit 7;;
    *) printf '%s\n' '"/nix/store/new-hello"';;
  esac
  ;;
*) exit 94;;
esac
"#;
    let brew_body = r#"#!/bin/sh
printf 'brew' >> "$OPTIONAL_READ_LOG"; printf ' <%s>' "$@" >> "$OPTIONAL_READ_LOG"; printf '\n' >> "$OPTIONAL_READ_LOG"
case "$1" in
formulae) [ "$#" = 1 ] || exit 95; printf 'hello\nworld\n';;
search)
  [ "$#" = 4 ] && [ "$2" = --formula ] && [ "$3" = -- ] && [ "$4" = '/hel.*/; literal' ] || exit 96
  printf 'hello\n';;
info)
  [ "$#" = 4 ] && [ "$2" = --json=v2 ] && [ "$3" = --installed ] && [ "$4" = --formula ] || exit 97
  printf '%s\n' '{"formulae":[{"name":"hello","full_name":"homebrew/core/hello","versions":{"stable":"1.0"}}]}' ;;
*) exit 98;;
esac
"#;
    for (name, body) in [("nix", nix_body), ("brew", brew_body)] {
        let path = directory.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // When: a fresh consumer process executes each operation through PATH resolution.
    for scenario in ["updates", "unchanged", "malformed", "failed", "brew"] {
        let log = directory.join(format!("{scenario}.argv"));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "optional_read_contracts_execute_real_command_boundaries",
                "--nocapture",
            ])
            .env("OPTIONAL_READ_SCENARIO", scenario)
            .env("OPTIONAL_READ_LOG", &log)
            .env("PATH", &directory)
            .output()
            .unwrap();
        // Then: results and failures come from the dispatched command, with exact argv captured.
        println!(
            "scenario={scenario} status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if log.exists() {
            println!("{}", fs::read_to_string(log).unwrap());
        }
        assert!(output.status.success());
    }
    fs::remove_dir_all(directory).unwrap();
}
