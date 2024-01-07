use proc_bitfield::{bitfield, BitRange};

use super::{
    dma::DMAChannels,
    game_pak::GamePak,
    irq::{IE, IF, IME},
    timer::Timers,
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

    /// Four incrementing 16-bit timers.
    pub timers: Timers,
    /// Four DMA transfer channels.
    pub dma_channels: DMAChannels,

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

    pub halt: bool,
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

            timers: Timers::default(),
            dma_channels: DMAChannels::default(),

            wram: box_arr![0x00; 0x48000],
            palette_ram: [0x00; 0x400],
            vram: box_arr![0x00; 0x18000],
            oam: [0x00; 0x400],
            game_pak: GamePak::default(),

            halt: false,
        }
    }
}

impl Bus {
    pub fn tick(&mut self, cycles: usize) {
        // TODO: APU, DMA, etc.
        self.ppu
            .cycle(&*self.vram, &self.palette_ram, &self.oam, &mut self.iff);
        self.timers.tick(&mut self.iff, cycles);
    }
}

impl Mcu for Bus {
    #[rustfmt::skip]
    fn read8(&mut self, address: u32) -> u8 {
        match address >> 24 {
            0x00 if address < 0x4000 => self.bios[address as usize],
            0x02 => self.wram[address as usize % 0x0004_0000],
            0x03 => self.wram[(address as usize % 0x0000_8000) + 0x0004_0000],
            0x04 => match address - 0x0400_0000 {
                addr @ 0x0000..=0x0050 => self.ppu.read8(addr),
                addr @ 0x0100..=0x010F => self.timers.read8(addr),
                0x0130 => self.key_input.keyinput() as u8,
                0x0131 => (self.key_input.keyinput() >> 8) as u8,
                0x0200 => self.ie.ie().bit_range::<0, 8>(),
                0x0201 => self.ie.ie().bit_range::<8, 16>(),
                0x0202 => self.iff.iff().bit_range::<0, 8>(),
                0x0203 => self.iff.iff().bit_range::<8, 16>(),
                0x0208 => self.ime.enabled() as u8,
                0x0209 => self.ime.ime().bit_range::<8, 16>(),
                0x020A => self.ime.ime().bit_range::<16, 24>(),
                0x020B => self.ime.ime().bit_range::<24, 32>(),
                _ => 0x00,
            },
            0x05 => self.palette_ram[address as usize % 0x400],
            0x06 => self.vram[address as usize % 0x0001_8000],
            0x07 => self.oam[address as usize % 0x400],
            0x08..=0x0D => self.game_pak.rom[address as usize - 0x0800_0000],
            0x0E..=0x0F => self.game_pak.sram[address as usize % 0x0001_0000],
            _ => 0,
        }
    }

    #[rustfmt::skip]
    fn write8(&mut self, address: u32, value: u8) {
        match address >> 24 {
            0x02 => self.wram[address as usize % 0x0004_0000] = value,
            0x03 => self.wram[(address as usize % 0x8000) + 0x0004_0000] = value,
            0x04 => match address - 0x0400_0000 {
                addr @ (0x0000..=0x003F | 0x0050..=0x0054) => self.ppu.write8(addr, value),
                addr @ 0x0100..=0x010F => self.timers.write8(addr, value),
                0x0200 => self.ie.set_ie(self.ie.0.set_bit_range::<0, 8>(value)),
                0x0201 => self.ie.set_ie(self.ie.0.set_bit_range::<8, 16>(value)),
                0x0202 => self.iff.set_iff((self.iff.iff() & !(value as u16)) & 0x3FFF),
                0x0203 => self.iff.set_iff((self.iff.iff() & !((value as u16) << 8)) & 0x3FFF),
                0x0208 => self.ime.set_enabled(value & 1 != 0),
                0x0209 => self.ime.set_ime(self.ime.0.set_bit_range::<8, 16>(value)),
                0x020A => self.ime.set_ime(self.ime.0.set_bit_range::<16, 24>(value)),
                0x020B => self.ime.set_ime(self.ime.0.set_bit_range::<24, 32>(value)),
                0x0301 => self.halt = (value >> 7) == 0,
                _ => {}
            },
            0x05 => self.palette_ram[address as usize % 0x400] = value,
            0x06 => self.vram[address as usize % 0x0001_8000] = value,
            0x07 => self.oam[address as usize % 0x400] = value,
            0x0E..=0x0F => self.game_pak.sram[address as usize % 0x0001_0000] = value,
            _ => {} // eprintln!("Write to ROM/unknown addr: {address:X}"),
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
