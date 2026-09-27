#[derive(Clone, Copy)]
pub(crate) enum Lang {
    Zh,
    En,
}

pub(crate) fn msg(lang: Lang, key: &str) -> &'static str {
    match (lang, key) {
        (Lang::Zh, "intro") => "=== Arch Linux 系统更新助手 ===",
        (Lang::En, "intro") => "=== Arch Linux System Update Assistant ===",
        (Lang::Zh, "desc") => "本脚本将执行以下操作：",
        (Lang::En, "desc") => "Actions to perform:",
        (Lang::Zh, "cancel") => "更新已取消。",
        (Lang::En, "cancel") => "Update cancelled.",
        (Lang::Zh, "confirm") => "请阅读上述新闻。确认使用 %s 开始更新吗？[Y/n] ",
        (Lang::En, "confirm") => "Proceed with %s? [Y/n] ",
        (Lang::Zh, "partial") => {
            "系统数据库已同步但升级失败，禁止执行部分升级；请解决冲突后重新运行。"
        }
        (Lang::En, "partial") => {
            "Databases were synchronized but the upgrade failed; do not perform a partial upgrade. Resolve the conflict and retry."
        }
        (Lang::Zh, "mirror_old") => "镜像列表已超过 30 天未更新。",
        (Lang::En, "mirror_old") => "The mirror list is older than 30 days.",
        (Lang::Zh, "mirror_fresh") => "镜像列表仍然新鲜。",
        (Lang::En, "mirror_fresh") => "The mirror list is fresh.",
        (Lang::Zh, "news_fail") => "获取新闻失败（网络或源错误）",
        (Lang::En, "news_fail") => "Failed to fetch news",
        (Lang::Zh, "force_confirm") => "新闻获取失败，是否强制更新？[y/N] ",
        (Lang::En, "force_confirm") => "News fetch failed. Force update? [y/N] ",
        (Lang::Zh, "mirror_skip") => "未找到镜像更新程序，跳过。",
        (Lang::En, "mirror_skip") => "Mirror updater not found; skipping.",
        (Lang::Zh, "snap") => "[0/6] 正在创建 Btrfs 快照...",
        (Lang::En, "snap") => "[0/6] Creating Btrfs Snapshot...",
        (Lang::Zh, "mirror_step") => "[1/6] 检查镜像源时效性...",
        (Lang::En, "mirror_step") => "[1/6] Checking mirrorlist age...",
        (Lang::Zh, "db_step") => "[2/6] 同步数据库并更新密钥环...",
        (Lang::En, "db_step") => "[2/6] Sync DB & Keyrings",
        (Lang::Zh, "update_step") => "[3/6] 正在升级系统...",
        (Lang::En, "update_step") => "[3/6] Upgrading system...",
        (Lang::Zh, "flatpak_step") => "[4/6] 检查 Flatpak 更新...",
        (Lang::En, "flatpak_step") => "[4/6] Checking Flatpak updates",
        (Lang::Zh, "grub_step") => "[5/6] 更新 GRUB 配置...",
        (Lang::En, "grub_step") => "[5/6] Updating GRUB",
        (Lang::Zh, "keyring_ok") => "数据库与密钥环已同步。",
        (Lang::En, "keyring_ok") => "DB & Keyrings synced.",
        (Lang::Zh, "keyring_warn") => "密钥环更新遇到问题，继续尝试系统更新...",
        (Lang::En, "keyring_warn") => "Keyring issues detected...",
        (Lang::Zh, "grub_ok") => "GRUB 更新成功。",
        (Lang::En, "grub_ok") => "GRUB updated.",
        (Lang::Zh, "grub_fail") => "GRUB 更新失败。",
        (Lang::En, "grub_fail") => "GRUB update failed.",
        _ => match lang {
            Lang::Zh => "未定义的系统更新消息。",
            Lang::En => "Undefined system update message.",
        },
    }
}

pub(crate) fn print_intro(lang: Lang) {
    println!("\x1b[1;36m{}\x1b[0m", msg(lang, "intro"));
    println!("{}", msg(lang, "desc"));
    if matches!(lang, Lang::Zh) {
        println!("  1. 获取最新 Arch Linux 新闻");
        println!("  2. 检查镜像源时效并按需更新");
        println!("  3. 同步软件数据库并更新 GPG 密钥环");
        println!("  4. 升级系统软件包 (交互模式防冲突)");
        println!("  5. 更新 Flatpak 应用");
        println!("  6. 重新生成 GRUB 配置文件");
    } else {
        println!("  1. Fetch latest Arch Linux news");
        println!("  2. Check mirrorlist age & Update if needed");
        println!("  3. Sync Pacman DB & Update GPG Keyrings");
        println!("  4. Upgrade system packages (Interactive)");
        println!("  5. Update Flatpak apps");
        println!("  6. Re-generate GRUB config");
    }
}

pub(crate) fn log_info(lang: Lang, text: &str) {
    println!(
        "\x1b[1;34m[{}]\x1b[0m {text}",
        if matches!(lang, Lang::Zh) {
            "消息"
        } else {
            "INFO"
        }
    );
}

pub(crate) fn log_success(lang: Lang, text: &str) {
    println!(
        "\x1b[1;32m[{}]\x1b[0m {text}",
        if matches!(lang, Lang::Zh) {
            "成功"
        } else {
            "OK"
        }
    );
}

pub(crate) fn log_warn(lang: Lang, text: &str) {
    println!(
        "\x1b[1;33m[{}]\x1b[0m {text}",
        if matches!(lang, Lang::Zh) {
            "注意"
        } else {
            "WARN"
        }
    );
}
