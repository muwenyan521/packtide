use anyhow::Result;

mod app;
mod check_updates;
mod cli;
mod commands;
mod downgrade;
mod install;
mod mirror_update;
mod model;
mod remove;
mod sources;
#[cfg(test)]
mod tests;
mod transaction;
mod ui;

fn main() -> Result<()> {
    app::run()
}
