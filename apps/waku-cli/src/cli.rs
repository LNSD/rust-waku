#[derive(Debug, Clone, clap::Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Commands {
    #[command(subcommand)]
    Relay(RelayCommand),
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum RelayCommand {
    Publish {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        pubsub_topic: String,
        #[arg(long)]
        content_topic: String,
        #[arg(long)]
        payload: String,
    },
    Subscribe {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        pubsub_topic: String,
        #[arg(long)]
        content_topic: String,
    },
}
