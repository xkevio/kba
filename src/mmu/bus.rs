use proc_bitfield::bitfield;

use super::{
    game_pak::GamePak,
    irq::{IE, IF, IME},
    Mcu,
};

use crate::{box_arr, ppu::lcd::Ppu};

pub struct Bus {
    /// BIOS - System ROM (needs to be provided).
    pub bios: &'static [u8],

    /// Picture Processing Unit, owns LCD IO registers.
    pub ppu: Ppu,
    /// Key Status.
    pub key_input: KEYINPUT,
    /// Interrupt Master Enable Register.
    pub ime: IME,
    /// Interrupt Enable Register.
    pub ie: IE,
    /// Interrupt Flag Request Register.
    pub iff: IF,

    /// On-board and On-chip Work RAM.
    pub wram: Box<[u8; 0x48000]>,
    /// BG/OBJ Palette Ram.
    pub palette_ram: [u8; 0x400],
    /// Video RAM.
    pub vram: Box<[u8; 0x18000]>,
    /// Object Attribute Memory.
    pub oam: [u8; 0x400],
    /// External Memory (Cartridge).
    pub game_pak: GamePak,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: include_bytes!("gba_bios.bin"),

            ppu: Ppu::default(),
            key_input: KEYINPUT(0xFFFF),
            ime: IME(0),
            ie: IE(0),
            iff: IF(0),

            wram: box_arr![0x00; 0x48000],
            palette_ram: [0x00; 0x400],
            vram: box_arr![0x00; 0x18000],
            oam: [0x00; 0x400],
            game_pak: GamePak::default(),
        }
    }
}

impl Mcu for Bus {
    #[rustfmt::skip]
    fn read8(&mut self, address: u32) -> u8 {
        // TODO: sram
        match address {
            0x0000..=0x3FFF => self.bios[address as usize],
            0x0200_0000..=0x02FF_FFFF => self.wram[address as usize % 0x0004_0000],
            0x0300_0000..=0x03FF_FFFF => self.wram[(address as usize % 0x0000_8000) + 0x3_FFFF],
            0x0400_0000..=0x0400_03FE => match address - 0x0400_0000 {
                0x0000..=0x0006 => self.ppu.read8(address),
                0x0130 => self.key_input.keyinput() as u8,
                0x0131 => (self.key_input.keyinput() >> 8) as u8,
                0x0200 => self.ie.ie() as u8,
                0x0201 => (self.ie.ie() >> 8) as u8,
                0x0202 => self.iff.iff() as u8,
                0x0203 => (self.iff.iff() >> 8) as u8,
                0x0208 => self.ime.enabled() as u8,
                0x0209 => (self.ime.ime() >> 8) as u8,
                0x020A => (self.ime.ime() >> 16) as u8,
                0x020B => (self.ime.ime() >> 24) as u8,
                _ => 0xFF,
            },
            0x0500_0000..=0x0500_03FF => self.palette_ram[address as usize - 0x0500_0000],
            0x0600_0000..=0x0601_7FFF => self.vram[address as usize - 0x0600_0000],
            0x0700_0000..=0x0700_03FF => self.oam[address as usize - 0x0700_0000],
            0x0800_0000..=0x0DFF_FFFF => self.game_pak.read8(address - 0x0800_0000),
            _ => 0,
        }
    }

    #[rustfmt::skip]
    fn write8(&mut self, address: u32, value: u8) {
        // TODO: sram
        match address {
            0x0200_0000..=0x02FF_FFFF => self.wram[address as usize % 0x0004_0000] = value,
            0x0300_0000..=0x03FF_FFFF => self.wram[(address as usize % 0x8000) + 0x3_FFFF] = value,
            0x0400_0000..=0x0400_03FE => match address - 0x0400_0000 {
                0x0000..=0x0006 => self.ppu.write8(address, value),
                0x0200 => self.ie.set_ie((self.ie.ie() & 0xFF00) | (value as u16)),
                0x0201 => self.ie.set_ie(((value as u16) << 8) | (self.ie.ie() & 0xFF)),
                0x0202 => self.iff.set_iff((self.iff.iff() & 0xFF00) | (value as u16)),
                0x0203 => self.iff.set_iff(((value as u16) << 8) | (self.iff.iff() & 0xFF)),
                0x0208 => self.ime.set_enabled(value & 1 != 0),
                0x0209 => self.ime.set_ime(((value as u32) << 8) | (self.ime.ime() & 0xFF)),
                0x020A => self.ime.set_ime(((value as u32) << 16) | (self.ime.ime() & 0xFFFF)),
                0x020B => self.ime.set_ime(((value as u32) << 24) | (self.ime.ime() & 0xFFFFFF)),
                _ => {}
            },
            0x0500_0000..=0x0500_03FF => self.palette_ram[address as usize - 0x0500_0000] = value,
            0x0600_0000..=0x0601_7FFF => self.vram[address as usize - 0x0600_0000] = value,
            0x0700_0000..=0x0700_03FF => self.oam[address as usize - 0x0700_0000] = value,
            _ => eprintln!("Write to ROM: {address:X}"),
        }
    }
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
