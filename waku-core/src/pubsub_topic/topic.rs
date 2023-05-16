///! Waku pubsub topic.
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct PubsubTopic(String);

impl PubsubTopic {
    /// Creates a new PubsubTopic from a string.
    pub fn new<S>(topic: S) -> PubsubTopic
    where
        S: Into<String>,
    {
        PubsubTopic(topic.into())
    }

    /// Return the length in bytes of this topic.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the length of this topic.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return a copy of this topic's byte representation.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }

    /// Return a reference to this topic's byte representation.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Extracts a string slice containing the entire topic.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PubsubTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for PubsubTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PubsubTopic {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(String::from(s)))
    }
}

impl From<&str> for PubsubTopic {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for PubsubTopic {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for PubsubTopic {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for PubsubTopic {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pubsub_topic_new_from_str() {
        // Given
        let topic_str = "test";

        // When
        let topic = PubsubTopic::new(topic_str);

        // Then
        assert_eq!(topic.to_string(), "test");
        assert_eq!(topic.len(), 4);
        assert!(!topic.is_empty());
        assert_eq!(topic.into_bytes(), vec![116, 101, 115, 116]);
    }

    #[test]
    fn test_pubsub_topic_from_str() {
        // Given
        let topic_str = "test";

        // When
        let topic = PubsubTopic::from_str(topic_str).unwrap();

        // Then
        assert_eq!(topic.to_string(), "test");
        assert_eq!(topic.len(), 4);
        assert!(!topic.is_empty());
        assert_eq!(topic.into_bytes(), vec![116, 101, 115, 116]);
    }

    #[test]
    fn test_pubsub_topic_from_string() {
        // Given
        let topic_str = "test".to_string();

        // When
        let topic = PubsubTopic::from(topic_str);

        // Then
        assert_eq!(topic.to_string(), "test");
        assert_eq!(topic.len(), 4);
        assert!(!topic.is_empty());
        assert_eq!(topic.into_bytes(), vec![116, 101, 115, 116]);
    }
}
