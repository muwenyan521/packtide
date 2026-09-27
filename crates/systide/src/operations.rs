use anyhow::Result;
use std::ffi::OsStr;
use system_tools_core::ExecutableResolver;

use crate::messages::{Lang, log_info, msg};

pub(crate) fn perform_update(manager: &str, lang: Lang) -> Result<()> {
    crate::snapshot::create(lang);
    log_info(lang, msg(lang, "db_step"));
    crate::mirror::check_age(lang)?;
    crate::update::run(manager, lang)?;
    crate::finish::run(lang)
}

pub(crate) fn detect_manager() -> Result<&'static str> {
    let resolver = ExecutableResolver::from_path(std::env::var_os("PATH").as_deref());
    ["paru", "yay"]
        .into_iter()
        .find(|v| resolver.resolve(OsStr::new(v)).is_some())
        .ok_or_else(|| anyhow::anyhow!("no AUR helper found (paru or yay)"))
}
