use anyhow::Result;
use std::io::IsTerminal;
use std::process::Command;
use system_tools_core::{TransactionAction, command_exists, run_status};

pub(crate) fn execute_package(
    helper: &str,
    action: TransactionAction,
    packages: &[String],
) -> Result<()> {
    let operation = match action {
        TransactionAction::Install => "-S",
        TransactionAction::Remove => "-Rns",
        TransactionAction::Upgrade | TransactionAction::Downgrade => {
            anyhow::bail!("unsupported package transaction action: {action:?}")
        }
    };
    let mut args = vec![operation];
    args.extend(packages.iter().map(String::as_str));
    let targets = packages.iter().map(String::as_str).collect::<Vec<_>>();
    print_summary(action.as_str(), helper, "helper-managed", &targets);
    run_status(helper, &args)?;
    Ok(())
}

pub(crate) fn execute_flatpak(packages: &[String]) -> Result<()> {
    let targets = packages.iter().map(String::as_str).collect::<Vec<_>>();
    print_summary("remove", "flatpak", "direct", &targets);
    run_status(
        "flatpak",
        &std::iter::once("uninstall")
            .chain(targets.iter().copied())
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

pub(crate) fn print_summary(action: &str, helper: &str, privilege: &str, targets: &[&str]) {
    let summary = summary_line(action, helper, privilege, targets);
    if std::io::stdout().is_terminal()
        && let Some((label, rest)) = summary.split_once(' ')
    {
        println!("\x1b[1;34m{label}\x1b[0m {rest}");
    } else {
        println!("{summary}");
    }
}

fn summary_line(action: &str, helper: &str, privilege: &str, targets: &[&str]) -> String {
    format!(
        "transaction: action={action} helper={helper} privilege={privilege} targets={}",
        targets.join(",")
    )
}

pub(crate) fn refresh_waybar_cache() {
    if command_exists("pkill") {
        let _ = Command::new("pkill")
            .args(["-SIGUSR1", "-f", "check-updates.sh"])
            .status();
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    if command_exists("flock") {
        let _ = Command::new("flock")
            .args(["-x", "-w", "120", "/tmp/waybar-updates.lock", "true"])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::summary_line;

    #[test]
    fn renders_transaction_summary_with_explicit_privilege_boundary() {
        assert_eq!(
            summary_line(
                "install",
                "paru",
                "helper-managed",
                &["core/bash", "aur/tool"]
            ),
            "transaction: action=install helper=paru privilege=helper-managed targets=core/bash,aur/tool"
        );
    }
}
