use crate::pubsub_topic::PubsubTopic;

const TOPIC_NAMED_SHARDING_PREFIX: &str = "/waku/2/";
const TOPIC_STATIC_SHARDING_PREFIX: &str = "/waku/2/rs/";

pub enum NsPubsubTopic {
    StaticSharding { cluster: u16, shard: u16 },
    NamedSharding(String),
    Raw(String),
}

fn parse_static_sharding(topic: &str) -> anyhow::Result<(u16, u16)> {
    let mut parts = topic
        .strip_prefix(TOPIC_STATIC_SHARDING_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid prefix"))?
        .split('/');

    let cluster = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing cluster index"))?
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("invalid cluster index"))?;
    let shard = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing shard index"))?
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("invalid shard index"))?;

    if parts.next().is_some() {
        anyhow::bail!("too many parts");
    }

    Ok((cluster, shard))
}

fn parse_named_sharding(topic: &str) -> anyhow::Result<String> {
    Ok(topic
        .strip_prefix(TOPIC_NAMED_SHARDING_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid prefix"))?
        .to_string())
}

impl NsPubsubTopic {
    pub fn new_static_sharding(cluster: u16, shard: u16) -> Self {
        Self::StaticSharding { cluster, shard }
    }

    pub fn new_named_sharding<S>(name: S) -> Self
    where
        S: Into<String>,
    {
        Self::NamedSharding(name.into())
    }

    pub fn raw<S>(name: S) -> Self
    where
        S: Into<String>,
    {
        Self::Raw(name.into())
    }
}

impl std::str::FromStr for NsPubsubTopic {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with(TOPIC_STATIC_SHARDING_PREFIX) {
            return parse_static_sharding(s)
                .map(|(cluster, shard)| Self::StaticSharding { cluster, shard });
        }

        if s.starts_with(TOPIC_NAMED_SHARDING_PREFIX) {
            return parse_named_sharding(s).map(Self::NamedSharding);
        }

        Ok(Self::Raw(s.to_string()))
    }
}

impl ToString for NsPubsubTopic {
    fn to_string(&self) -> String {
        match self {
            Self::StaticSharding { cluster, shard } => {
                format!("{}{}/{}", TOPIC_STATIC_SHARDING_PREFIX, cluster, shard)
            }
            Self::NamedSharding(name) => {
                format!("{}{}", TOPIC_NAMED_SHARDING_PREFIX, name)
            }
            Self::Raw(name) => name.clone(),
        }
    }
}

impl std::fmt::Debug for NsPubsubTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaticSharding { cluster, shard } => {
                write!(f, "StaticSharding(cluster={},shard={})", cluster, shard)
            }
            Self::NamedSharding(name) => write!(f, "NamedSharding({})", name),
            Self::Raw(name) => write!(f, "Raw({})", name),
        }
    }
}

impl TryFrom<PubsubTopic> for NsPubsubTopic {
    type Error = anyhow::Error;

    fn try_from(topic: PubsubTopic) -> Result<Self, Self::Error> {
        topic.to_string().parse()
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use crate::pubsub_topic::PubsubTopic;

    use super::*;

    #[test]
    fn test_parse_named_sharding_topic_valid_string() {
        // Given
        let topic = "/waku/2/abc";

        // When
        let name = parse_named_sharding(topic).unwrap();

        // Then
        assert_eq!(name, "abc");
    }

    #[test]
    fn test_parse_named_sharding_topic_invalid_prefix() {
        // Given
        let topic = "/waku/1/1/2";

        // When
        let result = parse_named_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_static_sharding_topic_valid_string() {
        // Given
        let topic = "/waku/2/rs/1/2";

        // When
        let (cluster, shard) = parse_static_sharding(topic).unwrap();

        // Then
        assert_eq!(cluster, 1);
        assert_eq!(shard, 2);
    }

    #[test]
    fn test_parse_static_sharding_topic_invalid_prefix() {
        // Given
        let topic = "/waku/2/1/2";

        // When
        let result = parse_static_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_static_sharding_topic_invalid_too_many_parts() {
        // Given
        let topic = "/waku/2/rs/1/2/3";

        // When
        let result = parse_static_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_static_sharding_topic_missing_cluster_index() {
        // Given
        let topic = "/waku/2/rs/";

        // When
        let result = parse_static_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_static_sharding_topic_invalid_cluster_index() {
        // Given
        let topic = "/waku/2/rs/1a/2";

        // When
        let result = parse_static_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_static_sharding_topic_missing_shard_index() {
        // Given
        let topic = "/waku/2/rs/1";

        // When
        let result = parse_static_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_static_sharding_topic_invalid_shard() {
        // Given
        let topic = "/waku/2/rs/1/2a";

        // When
        let result = parse_static_sharding(topic);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_ns_topic_static_sharding_from_str() {
        // Given
        let topic = "/waku/2/rs/1/2";

        // When
        let ns_topic = topic.parse::<NsPubsubTopic>().unwrap();

        // Then
        assert_matches!(
            ns_topic,
            NsPubsubTopic::StaticSharding {
                cluster: 1,
                shard: 2
            }
        );
    }

    #[test]
    fn test_ns_topic_named_sharding_from_str() {
        // Given
        let topic = "/waku/2/my-topic";

        // When
        let ns_topic = topic.parse::<NsPubsubTopic>().unwrap();

        // Then
        assert_matches!(ns_topic, NsPubsubTopic::NamedSharding(name) if name == "my-topic");
    }

    #[test]
    fn test_ns_topic_raw_from_str() {
        // Given
        let topic = "my-topic";

        // When
        let ns_topic = topic.parse::<NsPubsubTopic>().unwrap();

        // Then
        assert_matches!(ns_topic, NsPubsubTopic::Raw(name) if name == "my-topic");
    }

    #[test]
    fn test_ns_topic_static_sharding_to_string() {
        // Given
        let ns_topic = NsPubsubTopic::new_static_sharding(1, 2);

        // When
        let topic = ns_topic.to_string();

        // Then
        assert_eq!(topic, "/waku/2/rs/1/2");
    }

    #[test]
    fn test_ns_topic_named_sharding_to_string() {
        // Given
        let ns_topic = NsPubsubTopic::new_named_sharding("my-topic");

        // When
        let topic = ns_topic.to_string();

        // Then
        assert_eq!(topic, "/waku/2/my-topic");
    }

    #[test]
    fn test_ns_topic_raw_to_string() {
        // Given
        let ns_topic = NsPubsubTopic::raw("/waku/2/my-topic");

        // When
        let topic = ns_topic.to_string();

        // Then
        assert_eq!(topic, "/waku/2/my-topic");
    }

    #[test]
    fn test_ns_pubsub_topic_from_pubsub_topic_static_sharding() {
        // Given
        let ns_topic = PubsubTopic::new("/waku/2/rs/1/2");

        // When
        let topic = NsPubsubTopic::try_from(ns_topic).unwrap();

        // Then
        assert_matches!(
            topic,
            NsPubsubTopic::StaticSharding {
                cluster: 1,
                shard: 2
            }
        );
    }

    #[test]
    fn test_ns_pubsub_topic_from_pubsub_topic_named_sharding() {
        // Given
        let pubsub_topic = PubsubTopic::new("/waku/2/test-topic");

        // When
        let topic = NsPubsubTopic::try_from(pubsub_topic).unwrap();

        // Then
        assert_matches!(topic, NsPubsubTopic::NamedSharding(name) if name == "test-topic");
    }

    #[test]
    fn test_ns_pubsub_topic_from_pubsub_topic_raw() {
        // Given
        let topic = PubsubTopic::new("test");

        // When
        let ns_topic = NsPubsubTopic::try_from(topic).unwrap();

        // Then
        assert_matches!(ns_topic, NsPubsubTopic::Raw(name) if name == "test");
    }
}
