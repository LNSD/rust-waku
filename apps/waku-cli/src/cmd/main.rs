use crate::cmd::relay::RelayCommand;

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
