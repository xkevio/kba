use std::ops::{Index, IndexMut};

use super::{irq::IF, Mcu};
use proc_bitfield::ConvRaw;

/// Tuple struct to hold the four timers and manage read/writes.
#[derive(Default)]
pub struct Timers([Timer; 4]);

impl Timers {
    /// Tick all 4 timers based on their attributes and frequencies.
    ///
    /// Keep track of IDs for overflowing IRQ.
    pub fn tick(&mut self, iff: &mut IF, cycles: usize) {
        let mut tm_overflow = [false; 4];

        for id in 0..4 {
            if !self[id].start {
                continue;
            }

            let freq = match self[id].freq {
                Freq::F1 => 1,
                Freq::F64 => 64,
                Freq::F256 => 256,
                Freq::F1024 => 1024,
            };

            // Either tick up normally when the frequency is reached
            // or use Count-Up-Timing when previous timer overflows (not timer 0).
            if (!self[id].count_up && cycles % freq == 0)
                || (self[id].count_up && id > 0 && tm_overflow[id - 1])
            {
                tm_overflow[id] = self[id].tick();
            }

            if tm_overflow[id] {
                iff.set_timer(id);
            }
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
            0x010E => u16::from(self[3]),
            _ => 0,
        }
    }

    fn read8(&mut self, address: u32) -> u8 {
        match address & 1 == 0 {
            true => self.read16(address) as u8,
            false => (self.read16(address & 1) >> 8) as u8,
        }
    }

    fn write16(&mut self, address: u32, value: u16) {
        match address {
            0x0100 => self[0].reload = value,
            0x0102 => self[0].apply_tmr_cnt(value),
            0x0104 => self[1].reload = value,
            0x0106 => self[1].apply_tmr_cnt(value),
            0x0108 => self[2].reload = value,
            0x010A => self[2].apply_tmr_cnt(value),
            0x010C => self[3].reload = value,
            0x010E => self[3].apply_tmr_cnt(value),
            _ => unreachable!(),
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        // Make sure to "read" reload to modify it on write and not "read" counter.
        let [lo, hi] = match (address & !1) % 4 {
            0 => self[((address as usize & !1) - 0x0100) / 4].reload,
            _ => self.read16(address & !1),
        }
        .to_le_bytes();

        match address & 1 == 0 {
            true => self.write16(address, (hi as u16) << 8 | value as u16),
            false => self.write16(address & !1, (value as u16) << 8 | lo as u16),
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
    start: bool,
}

impl Timer {
    /// Update all the bits from the TMxCNT_H register.
    fn apply_tmr_cnt(&mut self, value: u16) {
        self.start = value & (1 << 7) != 0;
        self.irq = value & (1 << 6) != 0;
        self.count_up = value & (1 << 2) != 0;
        self.freq = Freq::try_from(value & 0x3).unwrap();
    }

    /// Tick timer by one; if overflow -> load `reload`, else just increase.
    /// Returns if timer has overflowed.
    fn tick(&mut self) -> bool {
        let (c, ov) = self.counter.overflowing_add(1);

        self.counter = match ov {
            true => self.reload,
            false => c,
        };

        return ov;
    }
}

impl From<Timer> for u16 {
    fn from(value: Timer) -> Self {
        0xFF00
            | (value.start as u16) << 7
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
