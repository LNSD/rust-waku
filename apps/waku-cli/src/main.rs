use clap::Parser;
use log::LevelFilter;
use log::{error, info};

use crate::cmd::{Cli, Commands, RelayCommand};

mod cmd;

async fn run_cmd(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Relay(RelayCommand::Publish(cmd_args)) => {
            cmd::relay::publish::run_cmd(cmd_args).await
        }
        Commands::Relay(RelayCommand::Subscribe(cmd_args)) => {
            cmd::relay::subscribe::run_cmd(cmd_args).await
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("ctrl-c received, shutting down");
            return Ok(());
        }

        res = run_cmd(cli) => {
            if let Err(e) = res {
                error!("Error: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
