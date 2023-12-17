use derivative::Derivative;
use proc_bitfield::{bitfield, BitRange};
use seq_macro::seq;

use crate::{
    gba::{LCD_HEIGHT, LCD_WIDTH},
    mmu::{irq::IF, Mcu},
};

use super::sprite::Sprite;

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

    #[derivative(Default(value = "vec![None; LCD_WIDTH * LCD_HEIGHT]"))]
    pub buffer: Vec<Option<u16>>,

    /// Current to-be-drawn line from the backgrounds, one for each prio.
    #[derivative(Default(value = "[[None; 512]; 4]"))]
    current_bg_line: [[Option<u16>; 512]; 4],
    /// Current to-be-drawn line for sprites, one for each prio.
    #[derivative(Default(value = "[[None; 512]; 4]"))]
    current_sprite_line: [[Option<u16>; 512]; 4],

    /// Up to 128 sprites from OAM for the current LY.
    current_sprites: Vec<Sprite>,

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
    pub fn cycle(&mut self, vram: &[u8], palette_ram: &[u8], oam: &[u8], iff: &mut IF) {
        match self.current_mode {
            Mode::HDraw => {
                if self.cycle > HDRAW_LEN {
                    self.scanline(vram, palette_ram, oam);

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

    // TODO: to combine update&draw methods for bgs and sprites.
    fn scanline(&mut self, vram: &[u8], palette_ram: &[u8], oam: &[u8]) {
        // Render backgrounds by either drawing text backgrounds or affine backgrounds.
        // If mode >= 3, we render directly into `self.buffer` and don't use the line draw function.
        self.update_bg_scanline(vram, palette_ram);
        if self.dispcnt.bg_mode() < 3 {
            // Render sprites by first collecting all sprites from OAM
            // that are on this line, then drawing them.
            self.current_sprites = Sprite::collect_obj_ly(oam, self.vcount.ly());
            self.render_sprite_line(vram, palette_ram);
            self.draw_bg_line();
        }
    }

    /// Render one scanline fully. (Mode 3 & 4 render directly into `self.buffer`)
    fn update_bg_scanline(&mut self, vram: &[u8], palette_ram: &[u8]) {
        // Render backgrounds.
        match self.dispcnt.bg_mode() {
            0 => {
                self.current_bg_line = [[None; 512]; 4];

                // Render backgrounds by iterating and
                // checking which are enabled via seq-macro.
                seq!(BG in 0..=3 {
                    if self.dispcnt.bg~BG() {
                        self.render_text_bg::<BG>(vram, palette_ram);
                    }
                });
            }
            3 => {
                let start = self.vcount.ly() as usize * LCD_WIDTH * 2;
                let line = &vram[start..(start + 480)];

                for (i, px) in line.chunks(2).enumerate() {
                    self.buffer[(start / 2) + i] = Some(u16::from_be_bytes([px[1], px[0]]));
                }
            }
            4 => {
                // TODO: this mode has two frames.
                let start = self.vcount.ly() as usize * LCD_WIDTH;
                let line = &vram[start..(start + LCD_WIDTH)];

                for (i, px) in line.iter().enumerate() {
                    let c0 = palette_ram[*px as usize * 2];
                    let c1 = palette_ram[*px as usize * 2 + 1];

                    self.buffer[start + i] = Some(u16::from_be_bytes([c1, c0]));
                }
            }
            _ => {}
        }
    }

    #[rustfmt::skip]
    fn render_text_bg<const BG: usize>(&mut self, vram: &[u8], palette_ram: &[u8]) {
        let bg_cnt = self.bgxcnt[BG];
        let bg_hofs = self.bgxhofs[BG];
        let bg_vofs = self.bgxvofs[BG];

        let y_off = (self.vcount.ly() as u16 + bg_vofs) % 256;
        let tile_data = bg_cnt.char_base_block() as u32 * 0x4000;

        for x in 0..LCD_WIDTH {
            let x_off = (x + bg_hofs as usize) % 256;
            let sbb_off = match bg_cnt.screen_size() {
                0 => 0,
                1 => ((x + bg_hofs as usize) % 512) / 256,
                2 => (y_off as usize % 512) / 256,
                3 => (((x + bg_hofs as usize) % 512) / 256) + ((y_off as usize % 512) / 256),
                _ => unreachable!(),
            } as u32;

            // Offset map_data screenblock if x > 255 or y > 255 depending on screen size.
            // Additionally, offset address by tile with x and y akin to (width * y + x).
            let map_data = bg_cnt.screen_base_block() as u32 * 0x800
                + sbb_off * 0x800
                + 2 * (32 * (y_off as u32 / 8) + (x_off as u32 / 8));

            let tile_id = ((vram[map_data as usize + 1] as u16) << 8) | (vram[map_data as usize]) as u16;
            let tile_start_addr = tile_data as usize + (tile_id as usize & 0x3FF) * (32 << bg_cnt.bpp() as usize);

            let h_flip = tile_id & (1 << 10) != 0;
            let v_flip = tile_id & (1 << 11) != 0;
            let pal_idx = tile_id >> 12;

            // Rendering starts here; based on the bits per pixel we address the palette RAM differently.
            // `tile_off` is a similar offset idea to the one in `map_data` but on pixel granularity.
            let x_flip = if h_flip { 7 - (x_off % 8) } else { x_off % 8 };
            let tile_off = if v_flip { 7 - (y_off as usize % 8) } else { y_off as usize % 8 } * 8 + x_flip;

            let tile_addr = tile_start_addr + tile_off / (2 >> bg_cnt.bpp() as usize);
            let (px_idx, px) = if !bg_cnt.bpp() {
                // 4 bits per pixel -> 16 palettes w/ 16 colors (1 byte holds the data for two neighboring pixels).
                let px_idx = ((vram[tile_addr] >> ((tile_off & 1) * 4)) & 0xF) as usize;

                (px_idx, u16::from_be_bytes([
                    palette_ram[(pal_idx as usize * 0x20) | px_idx * 2 + 1],
                    palette_ram[(pal_idx as usize * 0x20) | px_idx * 2],
                ]))
            } else {
                // 8 bits per pixel -> 1 palette w/ 256 colors
                let px_idx = vram[tile_addr] as usize;

                (px_idx, u16::from_be_bytes([
                    palette_ram[px_idx * 2 + 1],
                    palette_ram[px_idx * 2],
                ]))
            };

            if px_idx != 0 {
                self.current_bg_line[BG][x] = Some(px);
            }
        }
    }

    // TODO: rename functions
    /// Draw the scanline by placing it into the buffer. (For mode 0, 1, 2).
    fn draw_bg_line(&mut self) {
        let y = self.vcount.ly() as usize;

        // Get bits 8..=11 (const `END` parameter has to be one past) to get bg-enable bits.
        let is_bg_enabled: u8 = self.dispcnt.0.bit_range::<8, 12>();
        let mut bg_sorted = [0, 1, 2, 3];
        bg_sorted.sort_by_key(|i| self.bgxcnt[*i].prio());

        let mut render_line = vec![None; 512];

        // Draw all enabled background layers correctly sorted by priority.
        // Draw all the sprite layers on top of the backgrounds.
        for prio in bg_sorted {
            for x in 0..512 {
                let bg = (is_bg_enabled & (1 << prio) != 0)
                    .then_some(self.current_bg_line[prio][x])
                    .flatten();
                let sp = self.current_sprite_line[prio][x];

                render_line[x] = render_line[x].or(sp.or(bg));
            }
        }

        for x in 0..LCD_WIDTH {
            self.buffer[y * LCD_WIDTH + x] = render_line[x];
        }
    }

    /// Render all sprites in OAM at the current line.
    ///
    /// Sprite prio x > BG prio x for x in [0, 3].
    #[rustfmt::skip]
    fn render_sprite_line(&mut self, vram: &[u8], palette_ram: &[u8]) {
        if !self.dispcnt.obj() {
            return;
        }

        self.current_sprite_line = [[None; 512]; 4];
        for sprite in self.current_sprites.iter().rev() {
            if !sprite.rot_scale && !sprite.double_or_disable {
                let tile_amount = (sprite.width() / 8) * (sprite.height() / 8);
                let mut tiles = Vec::new();

                for i in 0..tile_amount {
                    let tile_nums = sprite.tile_id as u32
                        + if self.dispcnt.obj_char_vram_map() {
                            i as u32 * (sprite.bpp as u32 + 1)
                        } else {
                            let i = i % (sprite.width() / 8);
                            (tiles.len() as u32 / (sprite.width() as u32 / 8) * 0x20)
                                + (i as u32 * (sprite.bpp as u32 + 1))
                        };

                    tiles.push(tile_nums % 1024);
                }

                let y_diff = sprite.y.abs_diff(self.vcount.ly()) as usize;
                let y_start = match sprite.v_flip {
                    true => (sprite.height() as usize / 8) - (y_diff / 8) - 1,
                    false => y_diff / 8,
                } * (sprite.width() as usize / 8);
                let tiles_on_line = &tiles[y_start..(y_start + (sprite.width() as usize / 8))];

                for (x_idx, tile_id) in tiles_on_line.iter().enumerate() {
                    let tile_addr = 0x10000 + tile_id * 32;
                    let x_off = if sprite.h_flip { (tiles_on_line.len() - x_idx - 1) * 8 } else { x_idx * 8 };

                    for x in 0..8 {
                        let screen_x = (sprite.x as usize + x + x_off) % 512;
                        let tile_off = if sprite.v_flip { 7 - (y_diff % 8) } else { y_diff % 8 } * 8 + if sprite.h_flip { 7 - x } else { x };

                        let (px_idx, px) = if !sprite.bpp {
                            let px_idx = (vram[tile_addr as usize + tile_off / 2] >> ((tile_off & 1) * 4)) & 0xF;
                            (px_idx, u16::from_be_bytes([
                                palette_ram[0x200 + (sprite.pal_idx as usize * 0x20) | px_idx as usize * 2 + 1],
                                palette_ram[0x200 + (sprite.pal_idx as usize * 0x20) | px_idx as usize * 2],
                            ]))
                        } else {
                            let px_idx = vram[tile_addr as usize + tile_off];
                            (px_idx, u16::from_be_bytes([
                                palette_ram[0x200 + px_idx as usize * 2 + 1],
                                palette_ram[0x200 + px_idx as usize * 2],
                            ]))
                        };

                        if px_idx != 0 {
                            self.current_sprite_line[sprite.prio as usize][screen_x] = Some(px);
                        }
                    }
                }
            }

            // TODO: rot/scale later
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
