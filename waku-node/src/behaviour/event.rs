use libp2p::{identify, ping};

#[derive(Debug)]
pub enum Event {
    Ping(ping::Event),
    Identify(identify::Event),
    WakuRelay(waku_relay::Event),
}

impl From<ping::Event> for Event {
    fn from(event: ping::Event) -> Self {
        Event::Ping(event)
    }
}

impl From<identify::Event> for Event {
    fn from(event: identify::Event) -> Self {
        Event::Identify(event)
    }
}

impl From<waku_relay::Event> for Event {
    fn from(event: waku_relay::Event) -> Self {
        Event::WakuRelay(event)
    }
}
