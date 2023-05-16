///! Waku content topic.
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct ContentTopic(String);

impl ContentTopic {
    /// Create a new `ContentTopic` from a string.
    pub fn new<S>(topic: S) -> ContentTopic
    where
        S: Into<String>,
    {
        ContentTopic(topic.into())
    }

    /// Return the length in bytes of this `ContentTopic`.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the length of this `ContentTopic`.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Convert this `ContentTopic` into a byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }

    /// Return a byte slice of this `ContentTopic`'s content.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Return a string slice of this `ContentTopic`'s content.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ContentTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for ContentTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ContentTopic {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

impl From<&str> for ContentTopic {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for ContentTopic {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for ContentTopic {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for ContentTopic {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
