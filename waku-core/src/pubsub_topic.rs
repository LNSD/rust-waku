///! Waku pubsub topic.
use std::fmt;
use std::str::FromStr;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct PubsubTopic(String);

impl PubsubTopic {
    pub fn new<S>(topic: S) -> PubsubTopic
    where
        S: Into<String>,
    {
        PubsubTopic(topic.into())
    }

    /// Return the length in bytes of this content topic.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the length of this content topic.
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    /// Return a copy of this conte topic's byte representation.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.clone().into_bytes()
    }
}

impl fmt::Debug for PubsubTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string().fmt(f)
    }
}

impl fmt::Display for PubsubTopic {
    /// Convert a PubsubTopic to a string
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PubsubTopic {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl From<String> for PubsubTopic {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl Into<String> for PubsubTopic {
    fn into(self) -> String {
        self.0
    }
}
