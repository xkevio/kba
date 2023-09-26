use num_traits::Unsigned;

use super::{game_pak::GamePak, io::Io, Mcu};

pub struct Bus {
    pub bios: [u8; 0x4000],
    pub wram: [u8; 0x48000],
    pub io: Io,
    pub palette_ram: [u8; 0x400],
    pub vram: [u8; 0x18000],
    pub oam: [u8; 0x400],
    pub game_pak: GamePak,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: [0xFF; 0x4000],
            wram: [0xFF; 0x48000],
            io: Io::default(),
            palette_ram: [0xFF; 0x400],
            vram: [0xFF; 0x18000],
            oam: [0xFF; 0x400],
            game_pak: GamePak::default(),
        }
    }
}

impl Mcu for Bus {
    fn read<T: Unsigned>(&mut self, address: u32) -> T {
        todo!()
    }

    fn write<T: Unsigned>(&mut self, address: u32, value: T) {
        todo!()
    }
}
