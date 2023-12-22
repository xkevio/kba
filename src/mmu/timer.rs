use std::ops::{Index, IndexMut};

use proc_bitfield::ConvRaw;
use super::{Mcu, irq::IF};

/// Tuple struct to hold the four timers and manage read/writes.
#[derive(Default)]
pub struct Timers([Timer; 4]);

impl Timers {
    pub fn tick(&mut self, iff: &mut IF) {
        for id in 0..4 {
            // TODO: Implement tick.
            self[id].tick(id, iff);
        }
    }
}

impl Mcu for Timers {
    fn read16(&mut self, address: u32) -> u16 {
        match address {
            0x0100 => self[0].counter,
            0x0102 => u16::from(self[0]),
            0x0104 => self[1].counter,
            0x0106 => u16::from(self[1]),
            0x0108 => self[2].counter,
            0x010A => u16::from(self[2]),
            0x010C => self[3].counter,
            0x010F => u16::from(self[3]),
            _ => unreachable!()
        }
    }

    fn read8(&mut self, address: u32) -> u8 {
        match address % 2 == 0 {
            true => self.read16(address) as u8,
            false => (self.read16(address - 1) >> 8) as u8,
        }
    }

    fn write16(&mut self, address: u32, value: u16) {
        match address {
            0x0100 => self[0].reload = value,
            0x0102 => self[0].update(value),
            0x0104 => self[1].reload = value,
            0x0106 => self[1].update(value),
            0x0108 => self[2].reload = value,
            0x010A => self[2].update(value),
            0x010C => self[3].reload = value,
            0x010F => self[3].update(value),
            _ => unreachable!()
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        match address % 2 == 0 {
            true => self.write16(address, value as u16),
            false => self.write16(address, (value as u16) << 8),
        }
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

impl Timer {
    /// Update all the bits from the TMxCNT_H register.
    fn update(&mut self, value: u16) {
        let freq = value & 0x3;
        let count_up = value & (1 << 2) != 0;
        let irq = value & (1 << 6) != 0;
        let start_stop = value & (1 << 7) != 0;

        self.freq = Freq::try_from(freq).unwrap();
        self.count_up = count_up;
        self.irq = irq;
        self.start_stop = start_stop;
    }

    fn tick(&mut self, _id: usize, _iff: &mut IF) {
        todo!()
    }
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
