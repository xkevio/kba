use derivative::Derivative;
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

    pub bg0cnt: BGCONTROL,
    pub bg1cnt: BGCONTROL,
    pub bg2cnt: BGCONTROL,
    pub bg3cnt: BGCONTROL,

    /// Specifies the coordinate of the upperleft first visible dot of
    /// BG0 background layer, ie. used to scroll the BG0 area.
    pub bg0hofs: u16,
    pub bg0vofs: u16,
    /// Same as above BG0HOFS and BG0VOFS for BG1 respectively.
    pub bg1hofs: u16,
    pub bg1vofs: u16,
    /// Same as above BG0HOFS and BG0VOFS for BG2 respectively.
    pub bg2hofs: u16,
    pub bg2vofs: u16,
    /// Same as above BG0HOFS and BG0VOFS for BG3 respectively.
    pub bg3hofs: u16,
    pub bg3vofs: u16,

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
    pub fn cycle(&mut self, vram: &[u8], palette_ram: &[u8], iff: &mut IF) {
        match self.current_mode {
            Mode::HDraw => {
                if self.cycle > HDRAW_LEN {
                    self.scanline(vram, palette_ram);

                    self.dispstat.set_hblank(true);
                    self.current_mode = Mode::HBlank;

                    iff.set_hblank(self.dispstat.hblank_irq());
                }
            }
            Mode::HBlank => {
                if self.cycle > TOTAL_LEN {
                    self.vcount.set_ly(self.vcount.ly() + 1);

                    self.dispstat.set_hblank(false);
                    self.dispstat
                        .set_v_counter(self.vcount.ly() == self.dispstat.lyc());
                    iff.set_vcount(self.dispstat.v_counter() && self.dispstat.v_counter_irq());

                    if self.vcount.ly() >= 160 {
                        iff.set_vblank(self.dispstat.vblank_irq());
                        self.dispstat.set_vblank(true);
                        self.current_mode = Mode::VBlank;
                    } else {
                        self.current_mode = Mode::HDraw;
                    }

                    self.cycle = 0;
                }
            }
            Mode::VBlank => {
                // HBlank in DIPSTAT still gets set during VBlank.
                if self.cycle > HDRAW_LEN {
                    self.dispstat.set_hblank(true);
                }

                if self.cycle > TOTAL_LEN {
                    self.cycle = 0;
                    self.vcount.set_ly(self.vcount.ly() + 1);
                    self.dispstat.set_hblank(false);

                    if self.vcount.ly() == TOTAL_LINES {
                        self.vcount.set_ly(0);
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
                for (bg, enabled) in [
                    self.dispcnt.bg0(),
                    self.dispcnt.bg1(),
                    self.dispcnt.bg2(),
                    self.dispcnt.bg3(),
                ]
                .iter()
                .enumerate()
                {
                    if *enabled {
                        dbg!(bg);

                        let bg_cnt = BGCONTROL(self.read16(0x08 + (bg as u32 * 2)));
                        let bg_hofs = self.read16(0x10 + (bg as u32 * 4));
                        let bg_vofs = self.read16(0x12 + (bg as u32 * 4));

                        let y = self.vcount.ly();

                        let tiles_per_line = if bg_cnt.screen_size() % 2 == 0 { 32 } else { 64 }; 
                        let map_data = bg_cnt.screen_base_block() as u32 * 0x800 + (y as u32 / 8 * tiles_per_line);
                        let tile_data = bg_cnt.char_base_block() as u32 * 0x4000;

                        for (x, tile_entry) in (map_data..(map_data + tiles_per_line * 2)).step_by(2).enumerate() {
                            let tile_id = ((vram[tile_entry as usize] as u16) << 8) | (vram[tile_entry as usize + 1]) as u16;
                            let tile_start_addr = tile_data as usize + (tile_id as usize & 0x3FF) * ((bg_cnt.palettes() as usize + 1) * 32);
                            let palette = (tile_id >> 12) & 0xF;

                            if !bg_cnt.palettes() {
                                // 16/16, use palette num
                                let tile_start_addr_ly = tile_start_addr + (y as usize % 8);
                                for px in tile_start_addr_ly..(tile_start_addr_ly + 8) {
                                    let c0 = palette_ram[palette as usize * 0x20 + (vram[px] as usize * 2)];
                                    let c1 = palette_ram[palette as usize * 0x20 + (vram[px] as usize * 2 + 1)];

                                    if vram[px] == 0 { continue; }
                                    self.buffer[y as usize * LCD_WIDTH + x + px] = u16::from_be_bytes([c1, c0]);
                                }
                            } else {
                                // 256/1
                                let tile_start_addr_ly = tile_start_addr + (y as usize % 8);
                                for px in tile_start_addr_ly..(tile_start_addr_ly + 8) {
                                    let palette_index = vram[px];
                                    let c0 = palette_ram[palette_index as usize * 2];
                                    let c1 = palette_ram[palette_index as usize * 2 + 1];

                                    self.buffer[y as usize * LCD_WIDTH + x + px] = u16::from_be_bytes([c1, c0]);
                                }
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
            0x0008 => self.bg0cnt.bg_control() as u8,
            0x0009 => (self.bg0cnt.bg_control() >> 8) as u8,
            0x000A => self.bg1cnt.bg_control() as u8,
            0x000B => (self.bg1cnt.bg_control() >> 8) as u8,
            0x000C => self.bg2cnt.bg_control() as u8,
            0x000D => (self.bg2cnt.bg_control() >> 8) as u8,
            0x000E => self.bg3cnt.bg_control() as u8,
            0x000F => (self.bg3cnt.bg_control() >> 8) as u8,
            _ => 0,
        }
    }

    #[rustfmt::skip]
    fn write8(&mut self, address: u32, value: u8) {
        match address {
            0x0000 => self.dispcnt.set_dispcnt((self.dispcnt.0 & 0xFF00) | value as u16),
            0x0001 => {
                println!("write to upper byte of DISPCNT");
                self.dispcnt.set_dispcnt(((value as u16) << 8) | (self.dispcnt.0 & 0xFF))
            },
            0x0004 => self.dispstat.set_dispstat((self.dispstat.0 & 0xFF00) | value as u16),
            0x0005 => self.dispstat.set_dispstat(((value as u16) << 8) | (self.dispstat.0 & 0xFF)),
            0x0008 => self.bg0cnt.set_bg_control((self.bg0cnt.0 & 0xFF00) | value as u16),
            0x0009 => self.bg0cnt.set_bg_control((value as u16) << 8 | (self.bg0cnt.0 & 0xFF)),
            0x000A => self.bg1cnt.set_bg_control((self.bg1cnt.0 & 0xFF00) | value as u16),
            0x000B => self.bg1cnt.set_bg_control((value as u16) << 8 | (self.bg1cnt.0 & 0xFF)),
            0x000C => self.bg2cnt.set_bg_control((self.bg2cnt.0 & 0xFF00) | value as u16),
            0x000D => self.bg2cnt.set_bg_control((value as u16) << 8 | (self.bg2cnt.0 & 0xFF)),
            0x000E => self.bg3cnt.set_bg_control((self.bg3cnt.0 & 0xFF00) | value as u16),
            0x000F => self.bg3cnt.set_bg_control((value as u16) << 8 | (self.bg3cnt.0 & 0xFF)),
            0x0010 => self.bg0hofs = (self.bg0hofs & 0xFF00) | value as u16,
            0x0011 => self.bg0hofs = (self.bg0hofs & 0xFF) | ((value as u16) << 8),
            0x0012 => self.bg0vofs = (self.bg0vofs & 0xFF00) | value as u16,
            0x0013 => self.bg0vofs = (self.bg0vofs & 0xFF) | ((value as u16) << 8),
            0x0014 => self.bg1hofs = (self.bg1hofs & 0xFF00) | value as u16,
            0x0015 => self.bg1hofs = (self.bg1hofs & 0xFF) | ((value as u16) << 8),
            0x0016 => self.bg1vofs = (self.bg1vofs & 0xFF00) | value as u16,
            0x0017 => self.bg1vofs = (self.bg1vofs & 0xFF) | ((value as u16) << 8),
            0x0018 => self.bg2hofs = (self.bg2hofs & 0xFF00) | value as u16,
            0x0019 => self.bg2hofs = (self.bg2hofs & 0xFF) | ((value as u16) << 8),
            0x001A => self.bg2vofs = (self.bg2vofs & 0xFF00) | value as u16,
            0x001B => self.bg2vofs = (self.bg2vofs & 0xFF) | ((value as u16) << 8),
            0x001C => self.bg3hofs = (self.bg3hofs & 0xFF00) | value as u16,
            0x001D => self.bg3hofs = (self.bg3hofs & 0xFF) | ((value as u16) << 8),
            0x001E => self.bg3vofs = (self.bg3vofs & 0xFF00) | value as u16,
            0x001F => self.bg3vofs = (self.bg3vofs & 0xFF) | ((value as u16) << 8),
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
        pub palettes: bool @ 7,
        pub screen_base_block: u8 @ 8..=12,
        pub disp_area_overflow: bool @ 13,
        pub screen_size: u8 @ 14..=15,
    }
}
