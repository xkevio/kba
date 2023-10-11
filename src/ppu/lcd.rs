use derivative::Derivative;
use proc_bitfield::bitfield;

use crate::{
    gba::{LCD_HEIGHT, LCD_WIDTH},
    mmu::Mcu,
};

const HDRAW_LEN: u16 = 1006;
const TOTAL_LEN: u16 = 1232;
const TOTAL_LINES: u8 = 227;

#[derive(Derivative)]
#[derivative(Default)]
pub struct Ppu {
    pub dispcnt: DISPCNT,
    pub dispstat: DISPSTAT,
    pub vcount: VCOUNT,

    #[derivative(Default(value = "[0; LCD_WIDTH * LCD_HEIGHT]"))]
    pub buffer: [u16; LCD_WIDTH * LCD_HEIGHT],

    current_mode: Mode,
    cycle: u16,
}

#[derive(Default)]
enum Mode {
    #[default]
    HDraw,
    HBlank,
    VBlank,
}

impl Ppu {
    /// State machine that cycles through the modes and sets the right flags.
    pub fn cycle(&mut self, vram: &[u8], palette_ram: &[u8]) {
        match self.current_mode {
            Mode::HDraw => {
                if self.cycle > HDRAW_LEN {
                    if self.vcount.ly() >= 160 {
                        self.dispstat.set_vblank(true);
                        self.current_mode = Mode::VBlank;
                    } else {
                        self.scanline(vram, palette_ram);
                        self.dispstat.set_hblank(true);
                        self.current_mode = Mode::HBlank;
                    }
                }
            }
            Mode::HBlank => {
                if self.cycle > TOTAL_LEN {
                    self.vcount.set_ly((self.vcount.ly() + 1) % 228);
                    self.dispstat
                        .set_v_counter(self.vcount.ly() == self.dispstat.lyc());
                    self.dispstat.set_hblank(false);

                    if self.vcount.ly() >= 160 {
                        self.dispstat.set_vblank(true);
                        self.current_mode = Mode::VBlank;
                    } else {
                        self.current_mode = Mode::HDraw;
                    }

                    self.cycle = 0;
                }
            }
            Mode::VBlank => {
                if self.cycle > TOTAL_LEN {
                    self.cycle = 0;
                    self.vcount.set_ly(self.vcount.ly() + 1);

                    if self.vcount.ly() == TOTAL_LINES {
                        self.dispstat.set_vblank(false);

                        self.vcount.set_ly(0);
                        self.current_mode = Mode::HDraw;
                    }
                }
            }
        }

        self.cycle += 1;
    }

    /// Render one scanline fully.
    fn scanline(&mut self, vram: &[u8], palette_ram: &[u8]) {
        // println!("{:?}", palette_ram);
        match self.dispcnt.bg_mode() {
            3 => {
                let start = self.vcount.ly() as usize * LCD_WIDTH * 2;
                let line = &vram[start..(start + 480)];

                for (i, px) in line.chunks(2).enumerate() {
                    self.buffer[(start / 2) + i] = u16::from_be_bytes([px[1], px[0]]);
                }
            }
            4 => {
                // TODO: this mode has two frames.
                let start = self.vcount.ly() as usize * LCD_WIDTH;
                let line = &vram[start..(start + LCD_WIDTH)];

                for (i, px) in line.iter().enumerate() {
                    let c0 = palette_ram[*px as usize];
                    let c1 = palette_ram[*px as usize + 1];

                    self.buffer[start + i] = u16::from_be_bytes([c1, c0]);
                }
            }
            _ => unimplemented!(),
        }
    }
}

impl Mcu for Ppu {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            0x0000 => self.dispcnt.dispcnt() as u8,
            0x0001 => ((self.dispcnt.dispcnt() & 0xFF00) >> 8) as u8,
            0x0004 => self.dispstat.dispstat() as u8,
            0x0005 => ((self.dispstat.dispstat() & 0xFF00) >> 8) as u8,
            0x0006 => self.vcount.ly(),
            _ => 0,
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        match address {
            0x0000 => self
                .dispcnt
                .set_dispcnt((self.dispcnt.0 & 0xFF00) | value as u16),
            0x0001 => self
                .dispcnt
                .set_dispcnt(((value as u16) << 8) | (self.dispcnt.0 & 0xFF)),
            0x0004 => self
                .dispstat
                .set_dispstat((self.dispstat.0 & 0xFF00) | value as u16),
            0x0005 => self
                .dispstat
                .set_dispstat(((value as u16) << 8) | (self.dispstat.0 & 0xFF)),
            _ => {}
        }
    }
}

bitfield! {
    /// **DISPCNT - LCD Control** (r/w).
    #[derive(Clone, Copy, Default)]
    pub struct DISPCNT(pub u16) {
        pub dispcnt: u16 @ ..,
        pub bg_mode: u8 @ 0..=2,
        pub frame_select: bool @ 4,
        pub hblank_interval_free: bool @ 5,
        pub obj_char_vram_map: bool @ 6,
        pub forced_blank: bool @ 7,
        pub bg0: bool @ 8,
        pub bg1: bool @ 9,
        pub bg2: bool @ 10,
        pub bg3: bool @ 11,
        pub obj: bool @ 12,
        pub win0: bool @ 13,
        pub win1: bool @ 14,
        pub obj_win: bool @ 15,
    }
}

bitfield! {
    /// **DISPSTAT - General LCD Status** (r/w).
    #[derive(Clone, Copy, Default)]
    pub struct DISPSTAT(pub u16) {
        pub dispstat: u16 @ ..,
        pub vblank: bool @ 0,
        pub hblank: bool @ 1,
        pub v_counter: bool @ 2,
        pub vblank_irq: bool @ 3,
        pub hblank_irq: bool @ 4,
        pub v_counter_irq: bool @ 5,
        pub lyc: u8 @ 8..=15,
    }
}

bitfield! {
    /// **VCOUNT - Vertical Counter** (r).
    #[derive(Clone, Copy, Default)]
    pub struct VCOUNT(pub u16) {
        pub vcount: u16 @ ..,
        pub ly: u8 @ 0..=7,
    }
}
