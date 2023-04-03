use bitflags::bitflags;

bitflags! {
    /// The ENR `waku2` node capabilities bitfield.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct WakuEnrCapabilities: u8 {
        const RELAY     = 0b00000001;
        const STORE     = 0b00000010;
        const FILTER    = 0b00000100;
        const LIGHTPUSH = 0b00001000;
    }
}
