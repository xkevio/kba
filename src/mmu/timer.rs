use std::ops::{Index, IndexMut};

use proc_bitfield::ConvRaw;
use super::{Mcu, irq::IF};

/// Tuple struct to hold the four timers and manage read/writes.
#[derive(Default)]
pub struct Timers([Timer; 4]);

impl Timers {
    pub fn tick(&mut self, iff: &mut IF) {
        for id in 0..4 {
            // self[id].tick()...
        }
    }
}

impl Mcu for Timers {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            // Read counter and control for timer 0.
            0x0100 => self[0].counter as u8,
            0x0101 => (self[0].counter >> 8) as u8,
            0x0102 => u16::from(self[0]) as u8,
            0x0103 => (u16::from(self[0]) >> 8) as u8,

            // Read counter and control for timer 1.
            0x0104 => self[1].counter as u8,
            0x0105 => (self[1].counter >> 8) as u8,
            0x0106 => u16::from(self[1]) as u8,
            0x0107 => (u16::from(self[1]) >> 8) as u8,

            // Read counter and control for timer 2.
            0x0108 => self[2].counter as u8,
            0x0109 => (self[2].counter >> 8) as u8,
            0x010A => u16::from(self[2]) as u8,
            0x010B => (u16::from(self[2]) >> 8) as u8,

            // Read counter and control for timer 3.
            0x010C => self[3].counter as u8,
            0x010D => (self[3].counter >> 8) as u8,
            0x010E => u16::from(self[3]) as u8,
            0x010F => (u16::from(self[3]) >> 8) as u8,
            _ => unreachable!(),
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        todo!()
    }
}

impl Index<usize> for Timers {
    type Output = Timer;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Timers {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

/// 16-bit timer with all its attributes.
#[derive(Default, Clone, Copy)]
pub struct Timer {
    pub counter: u16,
    pub reload: u16,

    freq: Freq,
    count_up: bool,
    irq: bool,
    start_stop: bool,
}

impl From<Timer> for u16 {
    fn from(value: Timer) -> Self {
        0xFF00
            | (value.start_stop as u16) << 7
            | (value.irq as u16) << 6
            | (value.count_up as u16) << 2
            | value.freq as u16
    }
}

#[derive(ConvRaw, Default, Clone, Copy)]
enum Freq {
    #[default]
    F1,
    F64,
    F256,
    F1024,
}
