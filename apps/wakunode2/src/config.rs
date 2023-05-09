use clap::CommandFactory;
use config::Config;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Wakunode2Conf {
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub listen_addresses: Vec<String>,
    #[serde(default)]
    pub bootstrap_nodes: Vec<String>,
    #[serde(default)]
    pub keepalive: bool,

    #[serde(default)]
    pub relay: bool,
    #[serde(default)]
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, clap::Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Wakunode2Cli {
    #[clap(short = 'C', long, required = false, env = "WAKUNODE2_CONFIG_FILE")]
    config_file: String,
}

pub fn load() -> anyhow::Result<Wakunode2Conf> {
    let matches = Wakunode2Cli::command().get_matches();

    let mut conf_builder = Config::builder();
    if let Ok(Some(config_file)) = matches.try_get_one::<String>("config_file") {
        conf_builder =
            conf_builder.add_source(config::File::with_name(config_file.as_str()).required(false));
    }
    conf_builder = conf_builder.add_source(
        config::Environment::with_prefix("WAKUNODE2")
            .ignore_empty(true)
            .prefix_separator("_")
            .separator("_"),
    );

    let conf = conf_builder.build()?.try_deserialize::<Wakunode2Conf>()?;
    Ok(conf)
}
