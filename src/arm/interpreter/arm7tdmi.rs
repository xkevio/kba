use proc_bitfield::bitfield;

pub struct Arm7TDMI {
    pub regs: [u32; 16],
    pub cspr: Cspr,
}

pub enum State {
    Arm,
    Thumb,
}

impl From<u8> for State {
    fn from(value: u8) -> Self {
        if value == 0 {
            Self::Arm
        } else {
            Self::Thumb
        }
    }
}

bitfield! {
    // bits 8-9 arm11 only, 10-23 & 25-26 reserved, 24 unnecessary, 27 armv5 upwards.
    pub struct Cspr(pub u32) {
        pub cspr: u32 @ ..,
        /// Mode bits (fiq, irq, svc, user...)
        pub mode: u8 @ 0..=4,
        /// ARM (0) or THUMB (1)
        pub state: u8 [get State] @ 5; 1,
        pub fiq: bool @ 6,
        pub irq: bool @ 7,
        /// Overflow flag
        pub v: bool @ 28,
        /// Carry flag
        pub c: bool @ 29,
        /// Zero flag
        pub z: bool @ 30,
        /// Sign flag
        pub n: bool @ 31,
    }
}
