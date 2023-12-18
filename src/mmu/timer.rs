use proc_bitfield::ConvRaw;

use super::Mcu;

/// Tuple struct to hold the four timers and manage read/writes.
#[derive(Default)]
pub struct Timers([Timer; 4]);

impl Mcu for Timers {
    fn read8(&mut self, address: u32) -> u8 {
        todo!()
    }

    fn write8(&mut self, address: u32, value: u8) {
        todo!()
    }
}

/// 16-bit timer with all its attributes.
#[derive(Default)]
pub struct Timer {
    pub counter: u16,
    pub reload: u16,

    freq: Freq,
    count_up: bool,
    irq: bool,
    start_stop: bool,
}

#[derive(ConvRaw, Default)]
enum Freq {
    #[default]
    F1,
    F64,
    F256,
    F1024,
}

// bitfield! {
//     pub struct TMXCNT_H(pub u16) {
//         tmxcnt_h: u16 @ ..,
//         freq: u8 [try Freq] @ 0..=1,
//         count_up: bool @ 2,
//         irq_enable: bool @ 6,
//         start_stop: bool @ 7,
//     }
// }