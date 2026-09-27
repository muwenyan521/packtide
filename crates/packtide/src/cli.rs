use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "packtide",
    version,
    about = "Package and system maintenance tools"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<CommandLine>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandLine {
    CheckUpdates {
        #[arg(long)]
        refresh: bool,
    },
    Install {
        query: Vec<String>,
        #[arg(long, short = 'y')]
        refresh: bool,
        #[arg(long)]
        no_ai: bool,
    },
    Remove {
        query: Vec<String>,
    },
    MirrorUpdate {
        #[arg(short = 'c', long)]
        country: Option<String>,
    },
    Downgrade {
        query: Vec<String>,
    },
    Sysup {
        #[arg(short = 'l', long)]
        list: bool,
        #[arg(long, default_value = "auto")]
        ui_lang: String,
        #[arg(long, default_value = "official")]
        news_source: String,
        #[arg(long, default_value_t = 15)]
        count: usize,
    },
}

pub(crate) enum CompatibilityRoute {
    Install { query: Vec<String>, refresh: bool },
    Preview(Vec<String>),
}

pub(crate) fn compatibility_route(args: &[String]) -> Option<CompatibilityRoute> {
    let Some(first) = args.first() else {
        return Some(CompatibilityRoute::Install {
            query: Vec::new(),
            refresh: false,
        });
    };
    if first == "__preview" {
        return Some(CompatibilityRoute::Preview(args[1..].to_vec()));
    }
    let subcommands = [
        "check-updates",
        "install",
        "remove",
        "mirror-update",
        "downgrade",
        "sysup",
        "help",
    ];
    if subcommands.contains(&first.as_str()) {
        return None;
    }
    if first.starts_with('-') && !matches!(first.as_str(), "--refresh" | "-y" | "--no-ai") {
        return None;
    }
    let refresh = args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--refresh" | "-y"));
    let query = args
        .iter()
        .filter(|arg| !matches!(arg.as_str(), "--refresh" | "-y" | "--no-ai"))
        .cloned()
        .collect();
    Some(CompatibilityRoute::Install { query, refresh })
}

#[cfg(test)]
mod tests {
    use super::{CompatibilityRoute, compatibility_route};

    #[test]
    fn routes_default_and_query_to_install() {
        assert!(matches!(
            compatibility_route(&[]),
            Some(CompatibilityRoute::Install { query, refresh: false }) if query.is_empty()
        ));
        assert!(matches!(
            compatibility_route(&["package".to_owned()]),
            Some(CompatibilityRoute::Install { query, refresh: false }) if query == ["package"]
        ));
    }

    #[test]
    fn routes_legacy_refresh_flags_to_refreshing_install() {
        for flag in ["--refresh", "-y"] {
            assert!(matches!(
                compatibility_route(&[flag.to_owned(), "package".to_owned()]),
                Some(CompatibilityRoute::Install { query, refresh: true }) if query == ["package"]
            ));
        }
    }

    #[test]
    fn routes_no_ai_compatibility_flag_without_refresh() {
        assert!(matches!(
            compatibility_route(&["--no-ai".to_owned(), "package".to_owned()]),
            Some(CompatibilityRoute::Install { query, refresh: false }) if query == ["package"]
        ));
    }

    #[test]
    fn routes_combined_compatibility_flags_without_treating_them_as_query() {
        assert!(matches!(
            compatibility_route(&[
                "--refresh".to_owned(),
                "--no-ai".to_owned(),
                "package".to_owned()
            ]),
            Some(CompatibilityRoute::Install { query, refresh: true }) if query == ["package"]
        ));
    }

    #[test]
    fn recognizes_compatibility_flags_after_the_initial_query() {
        assert!(matches!(
            compatibility_route(&["package".to_owned(), "--refresh".to_owned()]),
            Some(CompatibilityRoute::Install { query, refresh: true }) if query == ["package"]
        ));
        assert!(matches!(
            compatibility_route(&["package".to_owned(), "--no-ai".to_owned()]),
            Some(CompatibilityRoute::Install { query, refresh: false }) if query == ["package"]
        ));
    }

    #[test]
    fn leaves_explicit_commands_for_clap_and_extracts_preview_args() {
        assert!(compatibility_route(&["remove".to_owned()]).is_none());
        assert!(compatibility_route(&["--unknown".to_owned()]).is_none());
        assert!(
            compatibility_route(&[
                "install".to_owned(),
                "--refresh".to_owned(),
                "--no-ai".to_owned(),
                "package".to_owned()
            ])
            .is_none()
        );
        assert!(matches!(
            compatibility_route(&["__preview".to_owned(), "install".to_owned(), "row".to_owned()]),
            Some(CompatibilityRoute::Preview(args)) if args == ["install", "row"]
        ));
    }
}
