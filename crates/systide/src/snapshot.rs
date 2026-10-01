use std::ffi::OsStr;
use system_tools_core::{ExecutableResolver, run_capture_path, run_status_path};

use crate::messages::{Lang, log_info, msg, msg_with};

pub(crate) fn create(lang: Lang) {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let Some(quicksave) = resolver.resolve(OsStr::new("quicksave")) else {
        println!("{}", msg(lang, "snapshot_tool_skip"));
        return;
    };
    let Some(findmnt) = resolver.resolve(OsStr::new("findmnt")) else {
        println!("{}", msg(lang, "snapshot_tool_skip"));
        return;
    };
    let filesystem = run_capture_path(&findmnt, &["-no", "FSTYPE", "/"], true)
        .ok()
        .map(|output| output.stdout.trim().to_owned());
    if filesystem.as_deref() == Some("btrfs") {
        log_info(lang, msg(lang, "snap"));
        if let Err(error) = run_status_path(&quicksave, &["-d", "quicksave-sysup"]) {
            eprintln!(
                "{}",
                msg_with(lang, "snapshot_failed", &[("error", error.to_string())])
            );
        }
    } else {
        println!("{}", msg(lang, "snapshot_skip"));
    }
}
