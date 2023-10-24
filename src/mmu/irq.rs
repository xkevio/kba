use proc_bitfield::{bitfield, ConvRaw};

#[derive(Debug, ConvRaw)]
pub enum Interrupt {
    VBlank,
    HBlank,
    VCount,
    Timer0,
    Timer1,
    Timer2,
    Timer3,
    Serial,
    DMA0,
    DMA1,
    DMA2,
    DMA3,
    Keypad,
    GamePak,
}

bitfield! {
    /// Interrupt Master Enable Register (r/w).
    #[derive(Default)]
    pub struct IME(pub u32) {
        pub ime: u32 @ ..,
        pub enabled: bool @ 0,
    }
}

bitfield! {
    /// Interrupt Enable Register (r/w).
    #[derive(Default)]
    pub struct IE(pub u16) {
        pub ie: u16 @ ..,
        pub vblank: bool @ 0,
        pub hblank: bool @ 1,
        pub vcount: bool @ 2,
        pub timer0: bool @ 3,
        pub timer1: bool @ 4,
        pub timer2: bool @ 5,
        pub timer3: bool @ 6,
        pub serial: bool @ 7,
        pub dma0: bool @ 8,
        pub dma1: bool @ 9,
        pub dma2: bool @ 10,
        pub dma3: bool @ 11,
        pub keypad: bool @ 12,
        pub gamepak: bool @ 13,
    }
}

bitfield! {
    /// Interrupt Request Flags (r/w).
    #[derive(Default)]
    pub struct IF(pub u16) {
        pub iff: u16 @ ..,
        pub vblank: bool @ 0,
        pub hblank: bool @ 1,
        pub vcount: bool @ 2,
        pub timer0: bool @ 3,
        pub timer1: bool @ 4,
        pub timer2: bool @ 5,
        pub timer3: bool @ 6,
        pub serial: bool @ 7,
        pub dma0: bool @ 8,
        pub dma1: bool @ 9,
        pub dma2: bool @ 10,
        pub dma3: bool @ 11,
        pub keypad: bool @ 12,
        pub gamepak: bool @ 13,
    }
}
