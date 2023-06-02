// Copyright 2020 Sigma Prime Pty Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::fmt;

use libp2p::PeerId;
use log::{debug, trace};

use crate::gossipsub::message_id::MessageId;
use crate::gossipsub::topic::TopicHash;

/// A message received by the gossipsub system and stored locally in caches..
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct CachedMessage {
    /// Id of the peer that published this message.
    pub source: Option<PeerId>,

    /// Content of the message. Its meaning is out of scope of this library.
    pub data: Vec<u8>,

    /// A random sequence number.
    pub sequence_number: Option<u64>,

    /// The topic this message belongs to
    pub topic: TopicHash,

    /// The signature of the message if it's signed.
    pub signature: Option<Vec<u8>>,

    /// The public key of the message if it is signed and the source [`PeerId`] cannot be inlined.
    pub key: Option<Vec<u8>>,

    /// Flag indicating if this message has been validated by the application or not.
    pub validated: bool,
}

/// CacheEntry stored in the history.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheEntry {
    mid: MessageId,
    topic: TopicHash,
}

/// MessageCache struct holding history of messages.
#[derive(Clone)]
pub(crate) struct MessageCache {
    msgs: HashMap<MessageId, (CachedMessage, HashSet<PeerId>)>,
    /// For every message and peer the number of times this peer asked for the message
    iwant_counts: HashMap<MessageId, HashMap<PeerId, u32>>,
    history: Vec<Vec<CacheEntry>>,
    /// The number of indices in the cache history used for gossiping. That means that a message
    /// won't get gossiped anymore when shift got called `gossip` many times after inserting the
    /// message in the cache.
    gossip: usize,
}

impl fmt::Debug for MessageCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageCache")
            .field("msgs", &self.msgs)
            .field("history", &self.history)
            .field("gossip", &self.gossip)
            .finish()
    }
}

/// Implementation of the MessageCache.
impl MessageCache {
    pub(crate) fn new(gossip: usize, history_capacity: usize) -> Self {
        MessageCache {
            gossip,
            msgs: HashMap::default(),
            iwant_counts: HashMap::default(),
            history: vec![Vec::new(); history_capacity],
        }
    }

    /// Put a message into the memory cache.
    ///
    /// Returns true if the message didn't already exist in the cache.
    pub(crate) fn put(&mut self, message_id: &MessageId, msg: CachedMessage) -> bool {
        match self.msgs.entry(message_id.clone()) {
            Entry::Occupied(_) => {
                // Don't add duplicate entries to the cache.
                false
            }
            Entry::Vacant(entry) => {
                let cache_entry = CacheEntry {
                    mid: message_id.clone(),
                    topic: msg.topic.clone(),
                };
                entry.insert((msg, HashSet::default()));
                self.history[0].push(cache_entry);

                trace!("Put message {:?} in mcache", message_id);
                true
            }
        }
    }

    /// Keeps track of peers we know have received the message to prevent forwarding to said peers.
    pub(crate) fn observe_duplicate(&mut self, message_id: &MessageId, source: &PeerId) {
        if let Some((message, originating_peers)) = self.msgs.get_mut(message_id) {
            // if the message is already validated, we don't need to store extra peers sending us
            // duplicates as the message has already been forwarded
            if message.validated {
                return;
            }

            originating_peers.insert(*source);
        }
    }

    /// Get a message with `message_id`
    #[cfg(test)]
    pub(crate) fn get(&self, message_id: &MessageId) -> Option<&CachedMessage> {
        self.msgs.get(message_id).map(|(message, _)| message)
    }

    /// Increases the iwant count for the given message by one and returns the message together
    /// with the iwant if the message exists.
    pub(crate) fn get_with_iwant_counts(
        &mut self,
        message_id: &MessageId,
        peer: &PeerId,
    ) -> Option<(&CachedMessage, u32)> {
        let iwant_counts = &mut self.iwant_counts;
        self.msgs.get(message_id).and_then(|(message, _)| {
            if !message.validated {
                None
            } else {
                Some((message, {
                    let count = iwant_counts
                        .entry(message_id.clone())
                        .or_default()
                        .entry(*peer)
                        .or_default();
                    *count += 1;
                    *count
                }))
            }
        })
    }

    /// Gets a message with [`MessageId`] and tags it as validated.
    /// This function also returns the known peers that have sent us this message. This is used to
    /// prevent us sending redundant messages to peers who have already propagated it.
    pub(crate) fn validate(
        &mut self,
        message_id: &MessageId,
    ) -> Option<(&CachedMessage, HashSet<PeerId>)> {
        self.msgs.get_mut(message_id).map(|(message, known_peers)| {
            message.validated = true;
            // Clear the known peers list (after a message is validated, it is forwarded and we no
            // longer need to store the originating peers).
            let originating_peers = std::mem::take(known_peers);
            (&*message, originating_peers)
        })
    }

    /// Get a list of [`MessageId`]s for a given topic.
    pub(crate) fn get_gossip_message_ids(&self, topic: &TopicHash) -> Vec<MessageId> {
        self.history[..self.gossip]
            .iter()
            .fold(vec![], |mut current_entries, entries| {
                // search for entries with desired topic
                let mut found_entries: Vec<MessageId> = entries
                    .iter()
                    .filter_map(|entry| {
                        if &entry.topic == topic {
                            let mid = &entry.mid;
                            // Only gossip validated messages
                            if let Some(true) = self.msgs.get(mid).map(|(msg, _)| msg.validated) {
                                Some(mid.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                // generate the list
                current_entries.append(&mut found_entries);
                current_entries
            })
    }

    /// Shift the history array down one and delete messages associated with the
    /// last entry.
    pub(crate) fn shift(&mut self) {
        for entry in self.history.pop().expect("history is always > 1") {
            if let Some((msg, _)) = self.msgs.remove(&entry.mid) {
                if !msg.validated {
                    // If GossipsubConfig::validate_messages is true, the implementing
                    // application has to ensure that Gossipsub::validate_message gets called for
                    // each received message within the cache timeout time."
                    debug!(
                        "The message with id {} got removed from the cache without being validated.",
                        &entry.mid
                    );
                }
            }
            trace!("Remove message from the cache: {}", &entry.mid);

            self.iwant_counts.remove(&entry.mid);
        }

        // Insert an empty vec in position 0
        self.history.insert(0, Vec::new());
    }

    /// Removes a message from the cache and returns it if existent
    pub(crate) fn remove(
        &mut self,
        message_id: &MessageId,
    ) -> Option<(CachedMessage, HashSet<PeerId>)> {
        //We only remove the message from msgs and iwant_count and keep the message_id in the
        // history vector. Zhe id in the history vector will simply be ignored on popping.

        self.iwant_counts.remove(message_id);
        self.msgs.remove(message_id)
    }
}
