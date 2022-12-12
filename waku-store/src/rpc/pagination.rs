use crate::pagination::PageCursor;
use crate::rpc::proto::waku::store::v2beta4::Index;

impl From<&Index> for PageCursor {
    fn from(cursor: &Index) -> Self {
        let store_timestamp = cursor.receiver_time;
        let digest = cursor.digest.clone();
        let pubsub_topic = cursor.pubsub_topic.clone();
        (store_timestamp, digest, pubsub_topic)
    }
}

impl From<PageCursor> for Index {
    fn from(cursor: PageCursor) -> Self {
        Index {
            digest: cursor.1.clone(),
            receiver_time: cursor.0,
            sender_time: cursor.0,
            pubsub_topic: cursor.2.clone(),
        }
    }
}
