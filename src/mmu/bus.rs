use crate::box_arr;

use super::{game_pak::GamePak, io::Io, Mcu};

pub struct Bus {
    pub bios: &'static [u8],
    pub wram: Box<[u8; 0x48000]>,
    pub io: Io,
    pub palette_ram: [u8; 0x400],
    pub vram: Box<[u8; 0x18000]>,
    pub oam: [u8; 0x400],
    pub game_pak: GamePak,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: include_bytes!("gba_bios.bin"),
            wram: box_arr!(0x00; 0x48000),
            io: Io::default(),
            palette_ram: [0x00; 0x400],
            vram: box_arr!(0x00; 0x18000),
            oam: [0x00; 0x400],
            game_pak: GamePak::default(),
        }
    }
}

impl Mcu for Bus {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            0x0000..=0x3FFF => self.bios[address as usize],
            0x0200_0000..=0x02FF_FFFF => self.wram[address as usize % 0x0004_0000],
            0x0300_0000..=0x03FF_FFFF => self.wram[(address as usize % 0x0000_8000) + 0x3_FFFF],
            0x0400_0000..=0x0400_03FE => self.io.read8(address - 0x0400_0000),
            0x0500_0000..=0x0500_03FF => self.palette_ram[address as usize - 0x0500_0000],
            0x0600_0000..=0x0601_7FFF => self.vram[address as usize - 0x0600_0000],
            0x0700_0000..=0x0700_03FF => self.oam[address as usize - 0x0700_0000],
            0x0800_0000..=0x0DFF_FFFF => self.game_pak.read8(address - 0x0800_0000),
            // TODO: sram
            _ => unreachable!("{}", format!("{address:X?}")),
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        match address {
            0x0200_0000..=0x02FF_FFFF => self.wram[address as usize % 0x0004_0000] = value,
            0x0300_0000..=0x03FF_FFFF => {
                self.wram[(address as usize % 0x0000_8000) + 0x3_FFFF] = value
            }
            0x0400_0000..=0x0400_03FE => self.io.write8(address - 0x0400_0000, value),
            0x0500_0000..=0x0500_03FF => self.palette_ram[address as usize - 0x0500_0000] = value,
            0x0600_0000..=0x0601_7FFF => self.vram[address as usize - 0x0600_0000] = value,
            0x0700_0000..=0x0700_03FF => self.oam[address as usize - 0x0700_0000] = value,
            // TODO: sram
            _ => unreachable!("{}", format!("{address:X?}")),
        }
    }
}
