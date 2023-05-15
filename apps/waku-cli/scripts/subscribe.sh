#AMS_PEER="/dns4/node-01.gc-us-central1-a.wakuv2.test.statusim.net/tcp/30303/p2p/16Uiu2HAmJb2e28qLXxT5kZxVUUoJt72EMzNGXB47Rxx5hw3q4YjS"
PEER="/dns4/localhost/tcp/10015/p2p/16Uiu2HAmKXw1VChPNBPKUttgW5mQAFATEMaLSEGhCVYvRWabjBHj"
PUBSUB_TOPIC="/waku/2/default-waku/proto"
CONTENT_TOPIC="/rust-waku/example/raw"

./target/debug/waku relay subscribe --peer $PEER --pubsub-topic $PUBSUB_TOPIC --content-topic $CONTENT_TOPIC
