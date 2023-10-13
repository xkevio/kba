use proc_bitfield::bitfield;

use crate::ppu::lcd::Ppu;

use super::Mcu;

#[derive(Default)]
pub struct Io {
    pub ppu: Ppu,
    pub key_input: KEYINPUT,
}

bitfield! {
    /// 0 = Pressed, 1 = Released
    pub struct KEYINPUT(pub u16) {
        pub keyinput: u16 @ ..,
        pub a: bool @ 0,
        pub b: bool @ 1,
        pub select: bool @ 2,
        pub start: bool @ 3,
        pub right: bool @ 4,
        pub left: bool @ 5,
        pub up: bool @ 6,
        pub down: bool @ 7,
        pub r: bool @ 8,
        pub l: bool @ 9,
    }
}

impl Default for KEYINPUT {
    fn default() -> Self {
        KEYINPUT(0xFF)
    }
}

impl Mcu for Io {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            0x0000..=0x0006 => self.ppu.read8(address),
            0x0130 => self.key_input.keyinput() as u8,
            0x0131 => ((self.key_input.keyinput() & 0xFF00) >> 8) as u8,
            _ => 0xFF,
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        match address {
            0x0000..=0x0006 => self.ppu.write8(address, value),
            _ => {}
        }
    }
}
