use proc_bitfield::bitfield;

use crate::mmu::Mcu;

#[derive(Default)]
pub struct Ppu {
    pub dispcnt: DISPCNT,
    pub dispstat: DISPSTAT,
    pub vcount: VCOUNT,

    cycle: u16,
}

impl Ppu {
    pub fn cycle(&mut self) {
        self.dispstat
            .set_vblank((160..227).contains(&self.vcount.ly()));
        self.dispstat.set_hblank(self.cycle > 1006);

        if self.cycle > 1232 {
            self.vcount.set_ly(self.vcount.ly() + 1);

            if self.vcount.ly() > 227 {
                self.vcount.set_ly(0);
            }

            self.dispstat
                .set_v_counter(self.vcount.ly() == self.dispstat.lyc());

            self.cycle = 0;
        }

        self.cycle += 1;
    }
}

impl Mcu for Ppu {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
            0x0000 => self.dispcnt.dispcnt() as u8,
            0x0001 => ((self.dispcnt.dispcnt() & 0xFF00) >> 8) as u8,
            0x0004 => {
                // stub vblank detection for now.
                // self.dispstat.set_vblank(!self.dispstat.vblank());
                self.dispstat.dispstat() as u8
            }
            0x0005 => ((self.dispstat.dispstat() & 0xFF00) >> 8) as u8,
            0x0006 => self.vcount.vcount() as u8,
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
