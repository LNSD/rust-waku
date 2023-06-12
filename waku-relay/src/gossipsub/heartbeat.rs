use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::{Stream, StreamExt};
use futures_ticker::Ticker;

pub(crate) struct Heartbeat {
    /// Heartbeat interval stream.
    ticker: Ticker,

    /// Number of heartbeats since the beginning of time; this allows us to amortize some resource
    /// clean up (e.g. backoff clean up).
    ticks: u64,
}

impl Heartbeat {
    pub(crate) fn new(interval: Duration, delay: Duration) -> Self {
        Self {
            ticker: Ticker::new_with_next(interval, delay),
            ticks: 0,
        }
    }
}

impl Stream for Heartbeat {
    type Item = u64;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.ticker.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(_)) => {
                self.ticks = self.ticks.wrapping_add(1);
                Poll::Ready(Some(self.ticks))
            }
            Poll::Ready(None) => Poll::Ready(None),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ticker.size_hint()
    }
}
