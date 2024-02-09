use derivative::Derivative;
use proc_bitfield::{bitfield, BitRange, ConvRaw};
use seq_macro::seq;

use crate::{
    bits,
    gba::{LCD_HEIGHT, LCD_WIDTH},
    mmu::{irq::IF, Mcu},
    set_bits,
};

use super::{
    blend, modify_brightness,
    sprite::{ObjMode, Sprite},
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

    /// Reference X, Y coordinates for affine background scrolling.
    pub bgxx: [i32; 2],
    pub bgxy: [i32; 2],

    /// Rotation/Scaling parameters for affine transformations.
    pub bgxpa: [i16; 2],
    pub bgxpb: [i16; 2],
    pub bgxpc: [i16; 2],
    pub bgxpd: [i16; 2],

    pub bldcnt: BLDCNT,
    pub bldalpha: BLDALPHA,
    pub bldy: BLDY,

    /// Window X horizontal and vertical dimensions.
    pub winxh: [u16; 2],
    pub winxv: [u16; 2],

    pub winin: WININ,
    pub winout: WINOUT,

    #[derivative(Default(value = "vec![None; LCD_WIDTH * LCD_HEIGHT]"))]
    pub buffer: Vec<Option<u16>>,

    /// Current to-be-drawn line from the backgrounds, one for each prio.
    #[derivative(Default(value = "[[None; 512]; 4]"))]
    current_bg_line: [[Option<u16>; 512]; 4],
    /// Current to-be-drawn line for sprites.
    #[derivative(Default(value = "[Obj::default(); 512]"))]
    current_sprite_line: [Obj; 512],

    /// Up to 128 sprites from OAM for the current LY.
    current_sprites: Vec<Sprite>,
    /// 32 groups of rotation/scaling data.
    current_rot_scale: Vec<(i16, i16, i16, i16)>,

    /// The internal reference point registers.
    internal_ref_xx: [i32; 2],
    internal_ref_xy: [i32; 2],

    // pub vid_capture: bool,
    pub prev_mode: Mode,
    pub current_mode: Mode,
    cycle: u16,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum Mode {
    #[default]
    HDraw,
    HBlank,
    VBlank,
}

#[derive(ConvRaw)]
pub enum ColorEffect {
    None,
    AlphaBlending,
    BrightnessIncrease,
    BrightnessDecrease,
}

#[derive(Default, Clone, Copy)]
struct Obj {
    px: Option<u16>,
    prio: u8,
    alpha: bool,
}

#[derive(Clone, Copy, PartialEq, Debug, PartialOrd, Ord, Eq)]
enum Window {
    Win0,
    Win1,
    ObjWin,
    WinOut,
}

impl Ppu {
    /// State machine that cycles through the modes and sets the right flags.
    pub fn cycle(&mut self, vram: &[u8], palette_ram: &[u8], oam: &[u8], iff: &mut IF) {
        match self.current_mode {
            Mode::HDraw => {
                if self.cycle > HDRAW_LEN {
                    self.scanline(vram, palette_ram, oam);

                    self.dispstat.set_hblank(true);
                    self.prev_mode = self.current_mode;
                    self.current_mode = Mode::HBlank;

                    if self.dispstat.hblank_irq() {
                        iff.set_hblank(true);
                    }
                }
            }
            Mode::HBlank => {
                if self.cycle > TOTAL_LEN {
                    // Internal reference point regs get incremented by dmx/dmy each scanline.
                    for bg in 0..2 {
                        self.internal_ref_xx[bg] += self.bgxpb[bg] as i32;
                        self.internal_ref_xy[bg] += self.bgxpd[bg] as i32;
                    }

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

                        self.prev_mode = self.current_mode;
                        self.current_mode = Mode::VBlank;
                    } else {
                        self.prev_mode = self.current_mode;
                        self.current_mode = Mode::HDraw;
                        // self.vid_capture = true;
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
                    // Reference points get copied to internal regs during VBlank.
                    self.internal_ref_xx = self.bgxx;
                    self.internal_ref_xy = self.bgxy;

                    self.cycle = 0;
                    self.dispstat.set_hblank(false);

                    self.vcount.set_ly(self.vcount.ly() + 1);
                    self.dispstat
                        .set_v_counter(self.vcount.ly() == self.dispstat.lyc());

                    if self.dispstat.v_counter() && self.dispstat.v_counter_irq() {
                        iff.set_vcount(true);
                    }

                    if self.vcount.ly() >= TOTAL_LINES {
                        self.vcount.set_ly(0); // vcount irq for ly = 0

                        self.dispstat
                            .set_v_counter(self.vcount.ly() == self.dispstat.lyc());

                        if self.dispstat.v_counter() && self.dispstat.v_counter_irq() {
                            iff.set_vcount(true);
                        }

                        self.dispstat.set_vblank(false);
                        self.prev_mode = self.current_mode;
                        self.current_mode = Mode::HDraw;
                        // self.vid_capture = true;
                    }
                }
            }
        }

        self.cycle += 1;
    }

    /// Render and draw one scanline fully.
    ///
    /// 1. `update_bg_scanline`:
    ///     - **if** mode < 3: `render_{text, affine}_bg` depending on mode.
    ///     - **else**: render directly into the buffer.
    ///
    /// 2. `render_sprite_line`:
    ///     - collect sprites from OAM.
    ///     - render them into according `current_sprite_line`.
    ///
    /// 3. `draw_line`:
    ///     - mix background and sprite lines according to their priorities.
    ///     - apply blending and other color effects.
    fn scanline(&mut self, vram: &[u8], palette_ram: &[u8], oam: &[u8]) {
        // Render backgrounds by either drawing text backgrounds or affine backgrounds.
        self.update_bg_scanline(vram, palette_ram);

        // Render sprites by first collecting all sprites from OAM
        // that are on this line, then drawing them. (todo: draw sprites for mode 3, 4, 5)
        self.current_sprites = Sprite::collect_obj_ly(oam, self.vcount.ly());
        self.current_rot_scale = Sprite::collect_rot_scale_params(oam);
        self.render_sprite_line(vram, palette_ram);

        // If mode >= 3, we render directly into `self.buffer`
        // and don't use the line draw function.
        if self.dispcnt.bg_mode() < 3 {
            self.draw_line(palette_ram);
        }
    }

    /// Render one background scanline fully. (Mode 3 & 4 render directly into `self.buffer`)
    fn update_bg_scanline(&mut self, vram: &[u8], palette_ram: &[u8]) {
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
            1 => {
                self.current_bg_line = [[None; 512]; 4];

                // Render backgrounds by iterating and
                // checking which are enabled via seq-macro.
                seq!(BG in 0..=2 {
                    if self.dispcnt.bg~BG() {
                        if BG < 2 {
                            self.render_text_bg::<BG>(vram, palette_ram);
                        } else {
                            self.render_affine_bg::<BG>(vram, palette_ram);
                        }
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

    #[rustfmt::skip]
    fn render_affine_bg<const BG: usize>(&mut self, vram: &[u8], palette_ram: &[u8]) {
        let bg_cnt = self.bgxcnt[BG];
        let screen_size = 128 << bg_cnt.screen_size();

        let mut bg_refx = self.internal_ref_xx[BG - 2] << 4 >> 4;
        let mut bg_refy = self.internal_ref_xy[BG - 2] << 4 >> 4;

        let (pa, pc) = (self.bgxpa[BG - 2] as i32, self.bgxpc[BG - 2] as i32);
        let tile_data = bg_cnt.char_base_block() as u32 * 0x4000;
        let screen_y = self.vcount.ly() as i32;

        // Screen space -> Texture space.
        for screen_x in 0..LCD_WIDTH {
            let mut tx = (bg_refx + screen_x as i32) >> 8;
            let mut ty = (bg_refy + screen_y) >> 8;

            bg_refx += pa;
            bg_refy += pc;

            // Transparency or Wraparound when display area overflow.
            if !bg_cnt.disp_area_overflow() {
                if !(0..screen_size).contains(&tx)
                    || !(0..screen_size).contains(&ty)
                {
                    continue;
                }
            } else {
                tx = tx.rem_euclid(screen_size);
                ty = ty.rem_euclid(screen_size);
            }

            // Why was this `2 * ...` here before?
            let map_data = bg_cnt.screen_base_block() as u32 * 0x800
                + 1 * ((screen_size as u32 / 8) * (ty as u32 / 8) + (tx as u32 / 8));

            let tile_id = vram[map_data as usize];
            let tile_start_addr = tile_data as usize + (tile_id as usize & 0x3FF) * 64;

            let tile_off = (ty as usize % 8) * 8 + (tx as usize % 8);
            let tile_addr = tile_start_addr + tile_off;

            let (px_idx, px) = {
                let px_idx = vram[tile_addr] as usize;

                (px_idx, u16::from_be_bytes([
                    palette_ram[px_idx * 2 + 1],
                    palette_ram[px_idx * 2]
                ]))
            };

            if px_idx != 0 {
                self.current_bg_line[BG][screen_x] = Some(px);
            }
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

        self.current_sprite_line = [Obj::default(); 512];
        for sprite in self.current_sprites.iter().rev() {
            if !sprite.rot_scale && sprite.double_or_disable {
                continue;
            }

            // Difference of y inside the sprite.
            let y = (sprite.y as i8 as i16).abs_diff(self.vcount.ly() as i16);

            // Use identity matrix for regular sprites and the correct params for affine.
            let (pa, pb, pc, pd) = match sprite.rot_scale {
                true => self.current_rot_scale[sprite.rot_scale_param as usize],
                false => (0x100, 0, 0, 0x100),
            };

            let width = sprite.width() << sprite.double_or_disable as u8;
            let height = sprite.height() << sprite.double_or_disable as u8;

            for spx in 0..width {
                let spx_off = sprite.x + spx as u16;

                // "Local" sprite coordinates within its bounding box.
                let lx = (spx_off - sprite.x) as i16;
                let ly = y as i16;

                // Transform into texture space with affine transformation.
                let mut tx = (pa * (lx - (width as i16 / 2)) + pb * (ly - (height as i16 / 2))) >> 8;
                let mut ty = (pc * (lx - (width as i16 / 2)) + pd * (ly - (height as i16 / 2))) >> 8;

                // Adjust sprite center.
                tx += ((width as i16) / 2) >> sprite.double_or_disable as i16;
                ty += ((height as i16) / 2) >> sprite.double_or_disable as i16;

                // Disable sprite wrapping and repeating itself.
                if tx < 0 || tx >= sprite.width() as i16 || ty < 0 || ty >= sprite.height() as i16 {
                    continue;
                }

                // Operate in "tile space".
                let tile_width = if sprite.h_flip && !sprite.rot_scale {
                    (sprite.width() as u16 / 8) - (tx as u16 / 8) - 1
                } else {
                    tx as u16 / 8
                };

                // Mapping modes for OAM tiles: two dimensional and one dimensional.
                // Two dimensional: upper row 0x00-0x1F, next row offset by 0x20.
                // One dimensional: upper row 0x00-0x1F, next row goes on normally.
                let vram_mapping_constant = if self.dispcnt.obj_char_vram_map() {
                    sprite.width() as u16 / 8 * (sprite.bpp as u16 + 1)
                } else {
                    0x20
                };

                let tile_id = sprite.tile_id
                    + tile_width as u16 * (sprite.bpp as u16 + 1)
                    + match sprite.v_flip && !sprite.rot_scale {
                        true => ((sprite.height() as u16 / 8) - (ty as u16 / 8) - 1) * vram_mapping_constant,
                        false => ty as u16 / 8 * vram_mapping_constant
                    };

                // In modes 3-5, only tile numbers 512-1023 may be used, lower memory is used for background.
                let tile_addr = match self.dispcnt.bg_mode() < 3 {
                    true => 0x10000 + (tile_id as usize % 1024) * 32,
                    false => 0x14000 + (tile_id as usize % 1024) * 32,
                };

                let screen_x = spx_off as usize % 512;
                let tile_off = if sprite.v_flip && !sprite.rot_scale { 7 - (ty as u16 % 8) } else { ty as u16 % 8 }
                    * 8 + if sprite.h_flip && !sprite.rot_scale { 7 - (tx as u16 % 8) } else { tx as u16 % 8 };

                let (px_idx, px) = if !sprite.bpp {
                    let px_idx = (vram[tile_addr as usize + tile_off as usize / 2] >> ((tile_off & 1) * 4)) & 0xF;
                    (px_idx, u16::from_be_bytes([
                        palette_ram[0x200 + (sprite.pal_idx as usize * 0x20) | px_idx as usize * 2 + 1],
                        palette_ram[0x200 + (sprite.pal_idx as usize * 0x20) | px_idx as usize * 2],
                    ]))
                } else {
                    let px_idx = vram[tile_addr as usize + tile_off as usize];
                    (px_idx, u16::from_be_bytes([
                        palette_ram[0x200 + px_idx as usize * 2 + 1],
                        palette_ram[0x200 + px_idx as usize * 2],
                    ]))
                };

                if px_idx != 0 {
                    self.current_sprite_line[screen_x] = Obj { 
                        px: Some(px), 
                        prio: sprite.prio, 
                        alpha: sprite.obj_mode == ObjMode::SemiTransparent 
                    };
                }
            }
        }
    }

    /// Draw the background scanline and sprites by placing it into the buffer. (For mode 0, 1, 2).
    fn draw_line(&mut self, palette_ram: &[u8]) {
        let y = self.vcount.ly() as usize;

        // Get bits 8..=11 to get bg-enable bits.
        let is_bg_enabled: u8 = bits!(self.dispcnt.0, 8..=11);
        let _backdrop = u16::from_le_bytes([palette_ram[0], palette_ram[1]]);

        let mut bg_sorted = [0, 1, 2, 3];
        let mut render_line = vec![None; 512];

        bg_sorted.sort_by_key(|i| self.bgxcnt[*i].prio());
        self.special_color_effect(palette_ram);

        // Draw all enabled background layers correctly sorted by priority.
        // Draw all the sprite layers on top of the backgrounds.
        for prio in bg_sorted {
            for x in 0..512 {
                let win = self.in_window(x, y);
                let sp = (self.current_sprite_line[x].prio == prio as u8)
                    .then_some(self.current_sprite_line[x].px)
                    .flatten();
                let bg = (is_bg_enabled & (1 << prio) != 0)
                    .then_some(self.current_bg_line[prio][x])
                    .flatten();

                // Windowing composition. TODO: OBJ Windows (+ SFX bit).
                let final_px = if self.dispcnt.win0() || self.dispcnt.win1() || self.dispcnt.obj_win() {
                    match win {
                        Window::Win0 | Window::Win1 => {
                            let offset = if win == Window::Win0 { 0 } else { 8 };
                            // Check if obj layer is enabled in window.
                            if self.winin.0 & (1 << (4 + offset)) != 0 {
                                // If obj layer is enabled, use pixel. If None (color 0), check for bg layer and use that.
                                sp.or_else(|| if self.winin.0 & (1 << (prio + offset)) != 0 { bg } else { None })
                            } else {
                                // Else, check if current bg layer is enabled and use that.
                                (self.winin.0 & (1 << (prio + offset)) != 0).then_some(bg).flatten()
                            }
                        },
                        Window::WinOut => {
                            // Check if either obj layer or bg layer is enabled for WINOUT.
                            // Use whichever is not None (either color 0 or layer disabled).
                            let sp_out = if self.winout.0 & (1 << 4) != 0 { sp } else { None };
                            let bg_out = if self.winout.0 & (1 << prio) != 0 { bg } else { None };

                            sp_out.or(bg_out)
                        },
                        Window::ObjWin => todo!(),
                    }
                } else {
                    sp.or(bg)
                };

                render_line[x] = render_line[x].or(final_px);
            }
        }

        for x in 0..LCD_WIDTH {
            self.buffer[y * LCD_WIDTH + x] = render_line[x];
        }
    }

    /// Apply special color effects such as alpha blending, whitening or darkening.
    ///
    /// "Inspired" by https://github.com/ITotalJustice/notorious_beeg/blob/master/src/core/ppu/render.cpp#L1325
    fn special_color_effect(&mut self, _palette_ram: &[u8]) {
        let src: u8 = bits!(self.bldcnt.0, 0..=5);
        let dst: u8 = bits!(self.bldcnt.0, 8..=13);

        let enabled_bgs: u8 = bits!(self.dispcnt.0, 8..=11);
        let Ok(color_effect) = self.bldcnt.color_effect() else {
            return;
        };

        for x in 0..512 {
            // Top two layers (pixel, prio, bg, obj_alpha).
            let mut layers = ([0u16; 2], [4u8; 2], [0usize; 2], false);

            let window = self.in_window(x, self.vcount.ly() as usize);
            // Check if layer is actually activated inside of a window before using it for blending.
            let layer_in_win = |layer: usize| {
                if self.dispcnt.win0() || self.dispcnt.win1() || self.dispcnt.obj_win() {
                    match window {
                        Window::Win0 => self.winin.0 & (1 << layer) != 0,
                        Window::Win1 => self.winin.0 & (1 << (layer + 8)) != 0,
                        Window::WinOut => self.winout.0 & (1 << layer) != 0,
                        Window::ObjWin => todo!(),
                    }
                } else {
                    true
                }
            };

            if self.dispcnt.obj() && layer_in_win(4) {
                if let Some(px) = self.current_sprite_line[x].px {
                    let obj_layer = 4;
                    let prio = self.current_sprite_line[x].prio;
                    let obj_alpha = self.current_sprite_line[x].alpha;

                    if prio < layers.1[0] {
                        // Swap top and bottom layer.
                        layers.0[1] = layers.0[0];
                        layers.1[1] = layers.1[0];
                        layers.2[1] = layers.2[0];
                        // Replace top layer with this new background.
                        layers.0[0] = px;
                        layers.1[0] = prio;
                        layers.2[0] = obj_layer;
                        layers.3 = obj_alpha;
                    } else if prio < layers.1[1] {
                        layers.0[1] = px;
                        layers.1[1] = prio;
                        layers.2[1] = obj_layer;
                    }
                }
            }

            for bg in (0..4).filter(|b| enabled_bgs & (1 << b) != 0 && layer_in_win(*b)) {
                if let Some(px) = self.current_bg_line[bg][x] {
                    if self.bgxcnt[bg].prio() < layers.1[0] {
                        // Swap top and bottom layer.
                        layers.0[1] = layers.0[0];
                        layers.1[1] = layers.1[0];
                        layers.2[1] = layers.2[0];
                        // Replace top layer with this new background.
                        layers.0[0] = px;
                        layers.1[0] = self.bgxcnt[bg].prio();
                        layers.2[0] = bg;
                        layers.3 = false;
                    } else if self.bgxcnt[bg].prio() < layers.1[1] {
                        layers.0[1] = px;
                        layers.1[1] = self.bgxcnt[bg].prio();
                        layers.2[1] = bg;
                    }
                }
            }

            // Obj Alpha.
            if layers.3 {
                if dst & (1 << layers.2[1]) != 0 {
                    layers.0[0] = blend(
                        layers.0[0],
                        layers.0[1],
                        self.bldalpha.eva(),
                        self.bldalpha.evb(),
                    );
                }
                self.current_sprite_line[x].px = self.current_sprite_line[x].px.map(|_| layers.0[0]);
            } else {
                match color_effect {
                    ColorEffect::AlphaBlending => {
                        if src & (1 << layers.2[0]) != 0 && dst & (1 << layers.2[1]) != 0 {
                            layers.0[0] = blend(
                                layers.0[0],
                                layers.0[1],
                                self.bldalpha.eva(),
                                self.bldalpha.evb(),
                            );
                        }
                    }
                    ColorEffect::BrightnessIncrease => {
                        if src & (1 << layers.2[0]) != 0 {
                            layers.0[0] = modify_brightness::<true>(layers.0[0], self.bldy.evy());
                        }
                    }
                    ColorEffect::BrightnessDecrease => {
                        if src & (1 << layers.2[0]) != 0 {
                            layers.0[0] = modify_brightness::<false>(layers.0[0], self.bldy.evy());
                        }
                    }
                    ColorEffect::None => return,
                }

                let layer_idx = if layers.2[0] == 4 { layers.2[1] } else { layers.2[0] };
                self.current_bg_line[layer_idx][x] = self.current_bg_line[layer_idx][x].map(|_| layers.0[0]);
            }
        }
    }

    fn in_window(&self, x: usize, y: usize) -> Window {
        for win in 0..2 {
            if self.dispcnt.0 & (1 << (13 + win)) == 0 {
                continue;
            }

            let x1 = (self.winxh[win] >> 8) as usize;
            let x2 = (self.winxh[win] & 0xFF) as usize;

            let y1 = (self.winxv[win] >> 8) as usize;
            let y2 = (self.winxv[win] & 0xFF) as usize;

            if x >= x1 && x < x2 && y >= y1 && y < y2 {
                return if win == 0 { Window::Win0 } else { Window::Win1 };
            }
        }

        Window::WinOut
    }
}

impl Mcu for Ppu {
    fn read16(&mut self, address: u32) -> u16 {
        match address {
            0x0000 => self.dispcnt.dispcnt(),
            0x0004 => self.dispstat.dispstat(),
            0x0006 => self.vcount.vcount(),
            0x0008 => self.bgxcnt[0].bg_control(),
            0x000A => self.bgxcnt[1].bg_control(),
            0x000C => self.bgxcnt[2].bg_control(),
            0x000E => self.bgxcnt[3].bg_control(),
            0x0048 => self.winin.winin(),
            0x004A => self.winout.winout(),
            0x0050 => self.bldcnt.bldcnt(),
            _ => 0,
        }
    }

    fn read8(&mut self, address: u32) -> u8 {
        match address & 1 == 0 {
            true => self.read16(address) as u8,
            false => (self.read16(address & !1) >> 8) as u8,
        }
    }

    fn write16(&mut self, address: u32, value: u16) {
        match address {
            0x0000 => self.dispcnt.set_dispcnt(value),
            0x0004 => self.dispstat.set_dispstat((value & !0b111) | self.dispstat.0 & 0b111),
            0x0008 => self.bgxcnt[0].set_bg_control(value),
            0x000A => self.bgxcnt[1].set_bg_control(value),
            0x000C => self.bgxcnt[2].set_bg_control(value),
            0x000E => self.bgxcnt[3].set_bg_control(value),
            0x0010 => self.bgxhofs[0] = value,
            0x0012 => self.bgxvofs[0] = value,
            0x0014 => self.bgxhofs[1] = value,
            0x0016 => self.bgxvofs[1] = value,
            0x0018 => self.bgxhofs[2] = value,
            0x001A => self.bgxvofs[2] = value,
            0x001C => self.bgxhofs[3] = value,
            0x001E => self.bgxvofs[3] = value,
            0x0020 => self.bgxpa[0] = value as i16,
            0x0022 => self.bgxpb[0] = value as i16,
            0x0024 => self.bgxpc[0] = value as i16,
            0x0026 => self.bgxpd[0] = value as i16,
            0x0028 => {
                set_bits!(self.bgxx[0], 0..=15, value);
                self.internal_ref_xx[0] = self.bgxx[0];
            }
            0x002A => {
                set_bits!(self.bgxx[0], 16..=27, value & 0xFFF);
                self.internal_ref_xx[0] = self.bgxx[0];
            }
            0x002C => {
                set_bits!(self.bgxy[0], 0..=15, value);
                self.internal_ref_xy[0] = self.bgxy[0];
            }
            0x002E => {
                set_bits!(self.bgxy[0], 16..=27, value & 0xFFF);
                self.internal_ref_xy[0] = self.bgxy[0];
            }
            0x0030 => self.bgxpa[1] = value as i16,
            0x0032 => self.bgxpb[1] = value as i16,
            0x0034 => self.bgxpc[1] = value as i16,
            0x0036 => self.bgxpd[1] = value as i16,
            0x0038 => {
                set_bits!(self.bgxx[1], 0..=15, value);
                self.internal_ref_xx[1] = self.bgxx[1];
            }
            0x003A => {
                set_bits!(self.bgxx[1], 16..=27, value & 0xFFF);
                self.internal_ref_xx[1] = self.bgxx[1];
            }
            0x003C => {
                set_bits!(self.bgxy[1], 0..=15, value);
                self.internal_ref_xy[1] = self.bgxy[1];
            }
            0x003E => {
                set_bits!(self.bgxy[1], 16..=27, value & 0xFFF);
                self.internal_ref_xy[1] = self.bgxy[1];
            }
            0x0040 => self.winxh[0] = value,
            0x0042 => self.winxh[1] = value,
            0x0044 => self.winxv[0] = value,
            0x0046 => self.winxv[1] = value,
            0x0048 => self.winin.set_winin(value),
            0x004A => self.winout.set_winout(value),
            0x0050 => self.bldcnt.set_bldcnt(value),
            0x0052 => self.bldalpha.set_bldalpha(value),
            0x0054 => self.bldy.set_bldy(value),
            _ => {}
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        let [lo, hi] = self.raw_read16(address & !1).to_le_bytes();
        match address & 1 == 0 {
            true => self.write16(address, (hi as u16) << 8 | value as u16),
            false => self.write16(address & !1, (value as u16) << 8 | lo as u16),
        }
    }

    /// Used in `write8` to get the internal value before modifying it.
    /// Also "reads" non-readable values but isn't used for bus access.
    fn raw_read16(&mut self, _address: u32) -> u16 {
        match _address {
            0x0000..=0x000F => self.read16(_address),
            0x0010 => self.bgxhofs[0],
            0x0012 => self.bgxvofs[0],
            0x0014 => self.bgxhofs[1],
            0x0016 => self.bgxvofs[1],
            0x0018 => self.bgxhofs[2],
            0x001A => self.bgxvofs[2],
            0x001C => self.bgxhofs[3],
            0x001E => self.bgxvofs[3],
            0x0020 => self.bgxpa[0] as u16,
            0x0022 => self.bgxpb[0] as u16,
            0x0024 => self.bgxpc[0] as u16,
            0x0026 => self.bgxpd[0] as u16,
            0x0028 => self.bgxx[0] as u16,
            0x002A => (self.bgxx[0] >> 16) as u16,
            0x002C => self.bgxy[0] as u16,
            0x002E => (self.bgxy[0] >> 16) as u16,
            0x0030 => self.bgxpa[1] as u16,
            0x0032 => self.bgxpb[1] as u16,
            0x0034 => self.bgxpc[1] as u16,
            0x0036 => self.bgxpd[1] as u16,
            0x0038 => self.bgxx[1] as u16,
            0x003A => (self.bgxx[1] >> 16) as u16,
            0x003C => self.bgxy[1] as u16,
            0x003E => (self.bgxy[1] >> 16) as u16,
            0x0040 => self.winxh[0],
            0x0042 => self.winxh[1],
            0x0044 => self.winxv[0],
            0x0046 => self.winxv[1],
            0x0048..=0x0050 => self.read16(_address),
            0x0052 => self.bldalpha.bldalpha(),
            0x0054 => self.bldy.bldy(),
            _ => 0,
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

bitfield! {
    /// **BLDCNT - Color Special Effects Selection** (r/w).
    #[derive(Clone, Copy, Default)]
    pub struct BLDCNT(pub u16) {
        pub bldcnt: u16 @ ..,
        pub bg0_first_px: bool @ 0,
        pub bg1_first_px: bool @ 1,
        pub bg2_first_px: bool @ 2,
        pub bg3_first_px: bool @ 3,
        pub obj_first_px: bool @ 4,
        pub bd_first_px: bool @ 5,
        pub color_effect: u8 [try ColorEffect] @ 6..=7,
        pub bg0_second_px: bool @ 8,
        pub bg1_second_px: bool @ 9,
        pub bg2_second_px: bool @ 10,
        pub bg3_second_px: bool @ 11,
        pub obj_second_px: bool @ 12,
        pub bd_second_px: bool @ 13,
    }
}

bitfield! {
    /// **BLDALPHA - Alpha Blending Coefficients** (w).
    #[derive(Clone, Copy, Default)]
    pub struct BLDALPHA(pub u16) {
        pub bldalpha: u16 @ ..,
        pub eva: u8 @ 0..=4,
        pub evb: u8 @ 8..=12,
    }
}

bitfield! {
    /// **BLDY - Brightness Coefficients** (w).
    #[derive(Clone, Copy, Default)]
    pub struct BLDY(pub u16) {
        pub bldy: u16 @ ..,
        pub evy: u8 @ 0..=4,
    }
}

bitfield! {
    /// **WININ - Control of Inside Windows** (r/w).
    #[derive(Clone, Copy, Default)]
    pub struct WININ(pub u16) {
        pub winin: u16 @ ..,
        pub win0_bg0: bool @ 0,
        pub win0_bg1: bool @ 1,
        pub win0_bg2: bool @ 2,
        pub win0_bg3: bool @ 3,
        pub win0_obj: bool @ 4,
        pub win0_col: bool @ 5,
        pub win1_bg0: bool @ 8,
        pub win1_bg1: bool @ 9,
        pub win1_bg2: bool @ 10,
        pub win1_bg3: bool @ 11,
        pub win1_obj: bool @ 12,
        pub win1_col: bool @ 13,
    }
}

bitfield! {
    /// **WINOUT - Control of Outside Windows & Obj** (r/w).
    #[derive(Clone, Copy, Default)]
    pub struct WINOUT(pub u16) {
        pub winout: u16 @ ..,
        pub win_bg0_out: bool @ 0,
        pub win_bg1_out: bool @ 1,
        pub win_bg2_out: bool @ 2,
        pub win_bg3_out: bool @ 3,
        pub win_obj_out: bool @ 4,
        pub win_col_out: bool @ 5,
        pub obj_win_bg0: bool @ 8,
        pub obj_win_bg1: bool @ 9,
        pub obj_win_bg2: bool @ 10,
        pub obj_win_bg3: bool @ 11,
        pub obj_win_obj: bool @ 12,
        pub obj_win_col: bool @ 13,
    }
}