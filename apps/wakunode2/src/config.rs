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

pub fn load(config_file: Option<String>) -> anyhow::Result<Wakunode2Conf> {
    let mut conf_builder = Config::builder();
    if let Some(config_file) = config_file {
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
