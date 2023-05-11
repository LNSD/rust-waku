use clap::Parser;
use log::info;
use log::LevelFilter;

use crate::cli::Cli;

mod app;
mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let conf = cli.into();
    let mut app = app::App::new(conf)?;
    app.setup().await?;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl-c received, shutting down");
                break;
            }

            ev = app.run() => {
                if let Ok(Some(event)) = ev {
                    info!("{event:?}");
                }
            }
        }
    }

    Ok(())
}
