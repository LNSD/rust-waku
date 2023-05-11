use clap::Parser;
use log::{info, LevelFilter};

mod app;
mod config;

#[derive(Debug, Clone, PartialEq, Eq, clap::Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'C', long, required = false, env = "WAKUNODE2_CONFIG_FILE")]
    pub config_file: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let conf = config::load(cli.config_file).unwrap();

    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let mut app = app::App::new(conf)?;
    app.setup().await?;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl-c received, shutting down");
                break;
            }

            ev = app.run() => {
                if let Some(event) = ev {
                    info!("{event:?}");
                }
            }
        }
    }

    Ok(())
}
