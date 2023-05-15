pub use publish::RelayPublishCmd;
pub use subscribe::RelaySubscribeCmd;

pub(crate) mod publish;
pub(crate) mod subscribe;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum RelayCommand {
    Publish(RelayPublishCmd),
    Subscribe(RelaySubscribeCmd),
}
