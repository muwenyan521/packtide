use anyhow::{Result, bail};
use clap::Parser;
use system_tools_core::require_privileged;

mod cli;
mod finish;
mod flow;
mod messages;
mod mirror;
mod news;
mod operations;
mod snapshot;
#[cfg(test)]
mod tests;
mod ui;
mod update;
use cli::Cli;
use flow::{confirm_force, confirm_start, language};
use messages::{msg, print_intro};
use news::{fetch_news, print_news};
use operations::{detect_manager, perform_update};
use ui::{collect_update_rows, show_update_list};

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.list_data {
        print!("{}", collect_update_rows()?);
        return Ok(());
    }
    if !matches!(cli.news_source.as_str(), "official" | "cn") {
        bail!("--news-source must be official or cn");
    }
    let lang = language(&cli.ui_lang)?;
    if cli.list {
        match show_update_list(lang)? {
            Some(true) => {}
            Some(false) => {
                println!("{}", msg(lang, "cancel"));
                return Ok(());
            }
            None => return Ok(()),
        }
    }
    print_intro(lang);
    messages::log_info(lang, msg(lang, "permission"));
    require_privileged()?;
    let manager = detect_manager()?;
    let news = fetch_news(&cli.news_source, cli.count, lang);
    if let Some(news) = news {
        if !news.is_empty() {
            print_news(lang, &news);
        }
    } else if !confirm_force(lang)? {
        println!("{}", msg(lang, "cancel"));
        return Ok(());
    }
    if !confirm_start(lang, manager)? {
        println!("{}", msg(lang, "cancel"));
        return Ok(());
    }
    perform_update(manager, lang)
}
