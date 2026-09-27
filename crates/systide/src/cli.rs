use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "systide", version, about = "System update orchestrator")]
pub(crate) struct Cli {
    #[arg(short = 'l', long)]
    pub(crate) list: bool,
    #[arg(long, hide = true)]
    pub(crate) list_data: bool,
    #[arg(long, default_value = "auto")]
    pub(crate) ui_lang: String,
    #[arg(long, default_value = "official")]
    pub(crate) news_source: String,
    #[arg(long, default_value_t = 15)]
    pub(crate) count: usize,
}
