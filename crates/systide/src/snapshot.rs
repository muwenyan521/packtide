use std::ffi::OsStr;
use system_tools_core::{ExecutableResolver, run_capture_path, run_status_path};

use crate::messages::{Lang, log_info, msg};

pub(crate) fn create(lang: Lang) {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    let Some(quicksave) = resolver.resolve(OsStr::new("quicksave")) else {
        return;
    };
    let Some(findmnt) = resolver.resolve(OsStr::new("findmnt")) else {
        return;
    };
    let filesystem = run_capture_path(&findmnt, &["-no", "FSTYPE", "/"], true)
        .ok()
        .map(|output| output.stdout.trim().to_owned());
    if filesystem.as_deref() == Some("btrfs") {
        log_info(lang, msg(lang, "snap"));
        if let Err(error) = run_status_path(&quicksave, &["-d", "quicksave-sysup"]) {
            eprintln!("snapshot skipped: {error}");
        }
    } else if matches!(lang, Lang::Zh) {
        println!("根文件系统不是 Btrfs，跳过快照。");
    }
}
