///! Waku content topic.
use std::fmt;
use std::str::FromStr;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct ContentTopic(String);

impl ContentTopic {
    pub fn new<S>(topic: S) -> ContentTopic
    where
        S: Into<String>,
    {
        ContentTopic(topic.into())
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

impl fmt::Debug for ContentTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string().fmt(f)
    }
}

impl fmt::Display for ContentTopic {
    /// Convert a ContentTopic to a string
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ContentTopic {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl From<String> for ContentTopic {
    fn from(s: String) -> Self {
        Self(s)
    }
}
