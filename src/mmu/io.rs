use crate::ppu::lcd::Ppu;

use super::Mcu;

#[derive(Default)]
pub struct Io {
    pub ppu: Ppu,
}

impl Mcu for Io {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            0x0000..=0x0006 => self.ppu.read8(address),
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
