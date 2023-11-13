use derivative::Derivative;
use itertools::Itertools;
use proc_bitfield::bitfield;

use crate::{
    gba::{LCD_HEIGHT, LCD_WIDTH},
    mmu::{irq::IF, Mcu},
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

    /// Background Control for background 0 - 3.
    pub bgxcnt: [BGCONTROL; 4],

    /// Specifies the coordinate of the upperleft first visible dot of
    /// BGx background layer, ie. used to scroll the BGx area.
    pub bgxhofs: [u16; 4],
    pub bgxvofs: [u16; 4],

    #[derivative(Default(value = "[0; 256 * 256]"))]
    pub buffer: [u16; 256 * 256],

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
    pub fn cycle(&mut self, vram: &[u8], palette_ram: &[u8], iff: &mut IF) {
        match self.current_mode {
            Mode::HDraw => {
                if self.cycle > HDRAW_LEN {
                    self.scanline(vram, palette_ram);

                    self.dispstat.set_hblank(true);
                    self.current_mode = Mode::HBlank;

                    if self.dispstat.hblank_irq() {
                        iff.set_hblank(true);
                    }
                }
            }
            Mode::HBlank => {
                if self.cycle > TOTAL_LEN {
                    self.cycle = 0;
                    self.dispstat.set_hblank(false);

                    self.vcount.set_ly(self.vcount.ly() + 1);
                    self.dispstat
                        .set_v_counter(self.vcount.ly() == self.dispstat.lyc());

                    if self.dispstat.v_counter() && self.dispstat.v_counter_irq() {
                        iff.set_vcount(true);
                    }

                    if self.vcount.ly() >= 160 {
                        if self.dispstat.vblank_irq() {
                            iff.set_vblank(true);
                        }
                        self.dispstat.set_vblank(true);
                        self.current_mode = Mode::VBlank;
                    } else {
                        self.current_mode = Mode::HDraw;
                    }
                }
            }
            Mode::VBlank => {
                // HBlank in DIPSTAT still gets set during VBlank.
                if self.cycle > HDRAW_LEN {
                    // if self.dispstat.hblank_irq() { iff.set_hblank(true); }
                    self.dispstat.set_hblank(true);
                }

                if self.cycle > TOTAL_LEN {
                    self.cycle = 0;
                    self.dispstat.set_hblank(false);

                    self.vcount.set_ly(self.vcount.ly() + 1);
                    self.dispstat
                        .set_v_counter(self.vcount.ly() == self.dispstat.lyc());

                    if self.dispstat.v_counter() && self.dispstat.v_counter_irq() {
                        iff.set_vcount(true);
                    }

                    if self.vcount.ly() == TOTAL_LINES {
                        self.vcount.set_ly(0); // todo: vcount irq for ly = 0
                        self.dispstat.set_vblank(false);
                        self.current_mode = Mode::HDraw;
                    }
                }
            }
        }

        self.cycle += 1;
    }

    /// Render one scanline fully.
    fn scanline(&mut self, vram: &[u8], palette_ram: &[u8]) {
        match self.dispcnt.bg_mode() {
            0 => {
                let sorted_bgs = [0, 1, 2, 3]
                    .iter()
                    .zip(self.bgxcnt.iter())
                    .sorted_by_key(|(_, bg)| 3 - bg.prio())
                    .collect_vec();

                #[rustfmt::skip]
                let bg_enable = [self.dispcnt.bg0(), self.dispcnt.bg1(), self.dispcnt.bg2(), self.dispcnt.bg3()];

                for (bg_i, bg_cnt) in sorted_bgs {
                    if bg_enable[*bg_i] {
                        // let _bg_hofs = self.bgxhofs[*bg_i];
                        // let _bg_vofs = self.bgxvofs[*bg_i];

                        let y = self.vcount.ly();

                        let tiles_per_line = if bg_cnt.screen_size() % 2 == 0 { 32 } else { 64 };
                        let map_data = bg_cnt.screen_base_block() as u32 * 0x800 + ((y as u32 / 8) * tiles_per_line * 2);
                        let tile_data = bg_cnt.char_base_block() as u32 * 0x4000;

                        for (x, tile_entry) in (map_data..(map_data + tiles_per_line * 2))
                            .step_by(2)
                            .enumerate()
                        {
                            let tile_id = ((vram[tile_entry as usize + 1] as u16) << 8) | (vram[tile_entry as usize]) as u16;
                            let tile_start_addr = tile_data as usize + (tile_id as usize & 0x3FF) * (32 << bg_cnt.bpp() as usize);
                            
                            let h_flip = tile_id & (1 << 10) != 0;
                            // let _v_flip = tile_id & (1 << 11) != 0;
                            let pal_idx = tile_id >> 12;

                            if !bg_cnt.bpp() {
                                // 4 bits per pixel -> 16 palettes w/ 16 colors (1 byte holds the data for two neighboring pixels).
                                let tile_start_addr_ly = tile_start_addr + (y as usize % 8) * 4;
                                for (i, px) in (tile_start_addr_ly..(tile_start_addr_ly + 4)).enumerate() {
                                    // Left pixel data is lower nibble of tile address.
                                    let px_left = u16::from_be_bytes([
                                        palette_ram[(pal_idx as usize * 0x20) | ((vram[px] as usize & 0xF) * 2 + 1)],
                                        palette_ram[(pal_idx as usize * 0x20) | (vram[px] as usize & 0xF) * 2],
                                    ]);

                                    // Right pixel data is upper nibble of tile address.
                                    let px_right = u16::from_be_bytes([
                                        palette_ram[(pal_idx as usize * 0x20) | ((vram[px] as usize >> 4) * 2 + 1)],
                                        palette_ram[(pal_idx as usize * 0x20) | (vram[px] as usize >> 4) * 2],
                                    ]); 

                                    let hori_x = if h_flip { 7 - i * 2 } else { i * 2 };
                                    let buf_idx = y as usize * 256 + (x * 8) + hori_x;

                                    // Color 0 of palette is "transparent".
                                    if vram[px] & 0xF != 0 {
                                        self.buffer[buf_idx] = px_left;
                                    }

                                    if vram[px] >> 4 != 0 {
                                        self.buffer[buf_idx + (1 - h_flip as usize * 2)] = px_right;
                                    }
                                }
                            } else {
                                // 8 bits per pixel -> 1 palette w/ 256 colors
                                todo!()
                            }
                        }
                    }
                }
            }
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
                    let c0 = palette_ram[*px as usize * 2];
                    let c1 = palette_ram[*px as usize * 2 + 1];

                    self.buffer[start + i] = u16::from_be_bytes([c1, c0]);
                }
            }
            _ => {}
        }
    }
}

// TODO: u16 r/w for IO
impl Mcu for Ppu {
    #[rustfmt::skip]
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            0x0000 => self.dispcnt.dispcnt() as u8,
            0x0001 => (self.dispcnt.dispcnt() >> 8) as u8,
            0x0004 => self.dispstat.dispstat() as u8,
            0x0005 => (self.dispstat.dispstat() >> 8) as u8,
            0x0006 => self.vcount.ly(),
            0x0008 => self.bgxcnt[0].bg_control() as u8,
            0x0009 => (self.bgxcnt[0].bg_control() >> 8) as u8,
            0x000A => self.bgxcnt[1].bg_control() as u8,
            0x000B => (self.bgxcnt[1].bg_control() >> 8) as u8,
            0x000C => self.bgxcnt[2].bg_control() as u8,
            0x000D => (self.bgxcnt[2].bg_control() >> 8) as u8,
            0x000E => self.bgxcnt[3].bg_control() as u8,
            0x000F => (self.bgxcnt[3].bg_control() >> 8) as u8,
            _ => 0,
        }
    }

    #[rustfmt::skip]
    fn write8(&mut self, address: u32, value: u8) {
        match address {
            0x0000 => self.dispcnt.set_dispcnt((self.dispcnt.0 & 0xFF00) | value as u16),
            0x0001 => self.dispcnt.set_dispcnt(((value as u16) << 8) | (self.dispcnt.0 & 0xFF)),
            0x0004 => self.dispstat.set_dispstat((self.dispstat.0 & 0xFF00) | (value & 0xF8) as u16),
            0x0005 => self.dispstat.set_dispstat(((value as u16) << 8) | (self.dispstat.0 & 0xFF)),
            0x0008 => self.bgxcnt[0].set_bg_control((self.bgxcnt[0].0 & 0xFF00) | value as u16),
            0x0009 => self.bgxcnt[0].set_bg_control((value as u16) << 8 | (self.bgxcnt[0].0 & 0xFF)),
            0x000A => self.bgxcnt[1].set_bg_control((self.bgxcnt[1].0 & 0xFF00) | value as u16),
            0x000B => self.bgxcnt[1].set_bg_control((value as u16) << 8 | (self.bgxcnt[1].0 & 0xFF)),
            0x000C => self.bgxcnt[2].set_bg_control((self.bgxcnt[2].0 & 0xFF00) | value as u16),
            0x000D => self.bgxcnt[2].set_bg_control((value as u16) << 8 | (self.bgxcnt[2].0 & 0xFF)),
            0x000E => self.bgxcnt[3].set_bg_control((self.bgxcnt[3].0 & 0xFF00) | value as u16),
            0x000F => self.bgxcnt[3].set_bg_control((value as u16) << 8 | (self.bgxcnt[3].0 & 0xFF)),
            0x0010 => self.bgxhofs[0] = (self.bgxhofs[0] & 0xFF00) | value as u16,
            0x0011 => self.bgxhofs[0] = (self.bgxhofs[0] & 0xFF) | ((value as u16) << 8),
            0x0012 => self.bgxvofs[0] = (self.bgxvofs[0] & 0xFF00) | value as u16,
            0x0013 => self.bgxvofs[0] = (self.bgxvofs[0] & 0xFF) | ((value as u16) << 8),
            0x0014 => self.bgxhofs[1] = (self.bgxhofs[1] & 0xFF00) | value as u16,
            0x0015 => self.bgxhofs[1] = (self.bgxhofs[1] & 0xFF) | ((value as u16) << 8),
            0x0016 => self.bgxvofs[1] = (self.bgxvofs[1] & 0xFF00) | value as u16,
            0x0017 => self.bgxvofs[1] = (self.bgxvofs[1] & 0xFF) | ((value as u16) << 8),
            0x0018 => self.bgxhofs[2] = (self.bgxhofs[2] & 0xFF00) | value as u16,
            0x0019 => self.bgxhofs[2] = (self.bgxhofs[2] & 0xFF) | ((value as u16) << 8),
            0x001A => self.bgxvofs[2] = (self.bgxvofs[2] & 0xFF00) | value as u16,
            0x001B => self.bgxvofs[2] = (self.bgxvofs[2] & 0xFF) | ((value as u16) << 8),
            0x001C => self.bgxhofs[3] = (self.bgxhofs[3] & 0xFF00) | value as u16,
            0x001D => self.bgxhofs[3] = (self.bgxhofs[3] & 0xFF) | ((value as u16) << 8),
            0x001E => self.bgxvofs[3] = (self.bgxvofs[3] & 0xFF00) | value as u16,
            0x001F => self.bgxvofs[3] = (self.bgxvofs[3] & 0xFF) | ((value as u16) << 8),
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

bitfield! {
    /// **BGxCNT - BG Control** (r/w).
    #[derive(Clone, Copy, Default)]
    pub struct BGCONTROL(pub u16) {
        pub bg_control: u16 @ ..,
        pub prio: u8 @ 0..=1,
        pub char_base_block: u8 @ 2..=3,
        pub mosaic: bool @ 6,
        pub bpp: bool @ 7,
        pub screen_base_block: u8 @ 8..=12,
        pub disp_area_overflow: bool @ 13,
        pub screen_size: u8 @ 14..=15,
    }
}
