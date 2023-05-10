use log::{info, LevelFilter};

mod app;
mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conf = config::load().unwrap();

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
