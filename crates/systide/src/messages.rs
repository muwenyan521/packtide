#[derive(Clone, Copy)]
pub(crate) enum Lang {
    Zh,
    En,
}

pub(crate) fn msg(lang: Lang, key: &str) -> &'static str {
    let source = match lang {
        Lang::Zh => include_str!("../locales/zh.txt"),
        Lang::En => include_str!("../locales/en.txt"),
    };
    source
        .lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|line| line.strip_prefix('='))
        })
        .unwrap_or(match lang {
            Lang::Zh => "未定义的系统更新消息。",
            Lang::En => "Undefined system update message.",
        })
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
