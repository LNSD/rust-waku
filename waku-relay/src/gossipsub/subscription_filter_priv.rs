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

use std::collections::{BTreeSet, HashMap, HashSet};

use log::debug;

use crate::gossipsub::TopicHash;
use crate::gossipsub::types::Subscription;

pub trait TopicSubscriptionFilter {
    /// Returns true iff the topic is of interest and we can subscribe to it.
    fn can_subscribe(&mut self, topic_hash: &TopicHash) -> bool;

    /// Filters a list of incoming subscriptions and returns a filtered set
    /// By default this deduplicates the subscriptions and calls
    /// [`Self::filter_incoming_subscription_set`] on the filtered set.
    fn filter_incoming_subscriptions<'a>(
        &mut self,
        subscriptions: &'a [Subscription],
        currently_subscribed_topics: &BTreeSet<TopicHash>,
    ) -> Result<HashSet<&'a Subscription>, String> {
        let mut filtered_subscriptions: HashMap<TopicHash, &Subscription> = HashMap::new();
        for subscription in subscriptions {
            use std::collections::hash_map::Entry::*;
            match filtered_subscriptions.entry(subscription.topic_hash.clone()) {
                Occupied(entry) => {
                    if entry.get().action != subscription.action {
                        entry.remove();
                    }
                }
                Vacant(entry) => {
                    entry.insert(subscription);
                }
            }
        }
        self.filter_incoming_subscription_set(
            filtered_subscriptions.into_values().collect(),
            currently_subscribed_topics,
        )
    }

    /// Filters a set of deduplicated subscriptions
    /// By default this filters the elements based on [`Self::allow_incoming_subscription`].
    fn filter_incoming_subscription_set<'a>(
        &mut self,
        mut subscriptions: HashSet<&'a Subscription>,
        _currently_subscribed_topics: &BTreeSet<TopicHash>,
    ) -> Result<HashSet<&'a Subscription>, String> {
        subscriptions.retain(|s| {
            if self.allow_incoming_subscription(s) {
                true
            } else {
                debug!("Filtered incoming subscription {:?}", s);
                false
            }
        });
        Ok(subscriptions)
    }

    /// Returns true iff we allow an incoming subscription.
    /// This is used by the default implementation of filter_incoming_subscription_set to decide
    /// whether to filter out a subscription or not.
    /// By default this uses can_subscribe to decide the same for incoming subscriptions as for
    /// outgoing ones.
    fn allow_incoming_subscription(&mut self, subscription: &Subscription) -> bool {
        self.can_subscribe(&subscription.topic_hash)
    }
}

//some useful implementers

/// Allows all subscriptions
#[derive(Default, Clone)]
pub struct AllowAllSubscriptionFilter {}

impl TopicSubscriptionFilter for AllowAllSubscriptionFilter {
    fn can_subscribe(&mut self, _: &TopicHash) -> bool {
        true
    }
}

/// Allows only whitelisted subscriptions
#[derive(Default, Clone)]
pub struct WhitelistSubscriptionFilter(pub HashSet<TopicHash>);

impl TopicSubscriptionFilter for WhitelistSubscriptionFilter {
    fn can_subscribe(&mut self, topic_hash: &TopicHash) -> bool {
        self.0.contains(topic_hash)
    }
}

/// Adds a max count to a given subscription filter
pub struct MaxCountSubscriptionFilter<T: TopicSubscriptionFilter> {
    pub filter: T,
    pub max_subscribed_topics: usize,
    pub max_subscriptions_per_request: usize,
}

impl<T: TopicSubscriptionFilter> TopicSubscriptionFilter for MaxCountSubscriptionFilter<T> {
    fn can_subscribe(&mut self, topic_hash: &TopicHash) -> bool {
        self.filter.can_subscribe(topic_hash)
    }

    fn filter_incoming_subscriptions<'a>(
        &mut self,
        subscriptions: &'a [Subscription],
        currently_subscribed_topics: &BTreeSet<TopicHash>,
    ) -> Result<HashSet<&'a Subscription>, String> {
        if subscriptions.len() > self.max_subscriptions_per_request {
            return Err("too many subscriptions per request".into());
        }
        let result = self
            .filter
            .filter_incoming_subscriptions(subscriptions, currently_subscribed_topics)?;

        use crate::gossipsub::types::SubscriptionAction::*;

        let mut unsubscribed = 0;
        let mut new_subscribed = 0;
        for s in &result {
            let currently_contained = currently_subscribed_topics.contains(&s.topic_hash);
            match s.action {
                Unsubscribe => {
                    if currently_contained {
                        unsubscribed += 1;
                    }
                }
                Subscribe => {
                    if !currently_contained {
                        new_subscribed += 1;
                    }
                }
            }
        }

        if new_subscribed + currently_subscribed_topics.len()
            > self.max_subscribed_topics + unsubscribed
        {
            return Err("too many subscribed topics".into());
        }

        Ok(result)
    }
}

/// Combines two subscription filters
pub struct CombinedSubscriptionFilters<T: TopicSubscriptionFilter, S: TopicSubscriptionFilter> {
    pub filter1: T,
    pub filter2: S,
}

impl<T, S> TopicSubscriptionFilter for CombinedSubscriptionFilters<T, S>
where
    T: TopicSubscriptionFilter,
    S: TopicSubscriptionFilter,
{
    fn can_subscribe(&mut self, topic_hash: &TopicHash) -> bool {
        self.filter1.can_subscribe(topic_hash) && self.filter2.can_subscribe(topic_hash)
    }

    fn filter_incoming_subscription_set<'a>(
        &mut self,
        subscriptions: HashSet<&'a Subscription>,
        currently_subscribed_topics: &BTreeSet<TopicHash>,
    ) -> Result<HashSet<&'a Subscription>, String> {
        let intermediate = self
            .filter1
            .filter_incoming_subscription_set(subscriptions, currently_subscribed_topics)?;
        self.filter2
            .filter_incoming_subscription_set(intermediate, currently_subscribed_topics)
    }
}

pub struct CallbackSubscriptionFilter<T>(pub T)
where
    T: FnMut(&TopicHash) -> bool;

impl<T> TopicSubscriptionFilter for CallbackSubscriptionFilter<T>
where
    T: FnMut(&TopicHash) -> bool,
{
    fn can_subscribe(&mut self, topic_hash: &TopicHash) -> bool {
        (self.0)(topic_hash)
    }
}

///A subscription filter that filters topics based on a regular expression.
pub struct RegexSubscriptionFilter(pub regex::Regex);

impl TopicSubscriptionFilter for RegexSubscriptionFilter {
    fn can_subscribe(&mut self, topic_hash: &TopicHash) -> bool {
        self.0.is_match(topic_hash.as_str())
    }
}
