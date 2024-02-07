use proc_bitfield::{bitfield, BitRange};

use super::{
    dma::{AddrControl, DMAChannels, StartTiming},
    game_pak::GamePak,
    irq::{IE, IF, IME},
    timer::Timers,
    Mcu,
};

use crate::{bits, box_arr, ppu::lcd::Ppu, set_bits};

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
    pub dma_in_progress: [bool; 4],

    pub cpu_r: [u32; 16],
    pub state: bool,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: include_bytes!("gba_bios.bin"),

            ppu: Ppu::default(),
            key_input: KEYINPUT(0x03FF),
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
            dma_in_progress: [false; 4],

            cpu_r: [0; 16],
            state: false,
        }
    }
}

impl Bus {
    pub fn tick(&mut self, cycles: &mut usize) {
        self.ppu.cycle(
            &*self.vram, 
            &self.palette_ram, 
            &self.oam, 
            &mut self.iff,
        );
        self.timers.tick(&mut self.iff, *cycles);

        /* 
        The following DMA checks can still be optimized if they are only called
        directly when HBlank or VBlank happens, instead this still checks stuff
        every cycle but doesn't run it every cycle.

        Similar for Immediate DMA. Problem is getting `self.dma_transfer` from
        the borrow-checker into the PPU state machine.
        */

        // If any DMA is in progress
        if self.dma_in_progress.iter().all(|c| *c == false) {
            // On state/mode change.
            if self.ppu.prev_mode != self.ppu.current_mode {
                use crate::ppu::lcd::Mode;
                match self.ppu.current_mode {
                    Mode::HBlank => self.dma_transfer(StartTiming::HBlank, cycles),
                    Mode::VBlank => self.dma_transfer(StartTiming::VBlank, cycles),
                    Mode::HDraw => {},
                }
    
                self.ppu.prev_mode = self.ppu.current_mode;
            }
    
            // On enable transition for immediate DMAs.
            if (0..4).any(|ch| self.dma_channels[ch].enable_edge()) {
                self.dma_transfer(StartTiming::Immediate, cycles);
            }
        }
    }

    fn dma_transfer(&mut self, dma_type: StartTiming, cycles: &mut usize) {
        let channels = self.dma_channels;

        for ch in 0..4 {
            let src_addr_control = channels[ch].src_addr_ctrl;
            let dst_addr_control = channels[ch].dst_addr_ctrl;
            let start_timing = channels[ch].start_timing;

            let addr_delta = if channels[ch].transfer_type { 4 } else { 2 };

            let mut src_addr = channels[ch].src;
            let mut dst_addr = channels[ch].dst;
            let word_count = match channels[ch].word_count == 0 {
                true if ch == 3 => 0xFFFF,
                true => 0x3FFF,
                false => channels[ch].word_count,
            };

            // TODO: Special start (Video Capture) timing and wow, this would be nicer with a scheduler.
            if channels[ch].enable {
                if start_timing == dma_type
                    || start_timing == dma_type && self.ppu.dispstat.hblank() && !self.ppu.dispstat.vblank()
                    || start_timing == dma_type && self.ppu.dispstat.vblank() 
                    // || start_timing == StartTiming::Special && ch == 3 && self.ppu.vcount.ly() >= 2 && self.ppu.vcount.ly() <= 162 && self.ppu.vid_capture
                {
                    self.dma_in_progress[ch] = true;

                    for _ in 0..word_count {
                        if channels[ch].transfer_type {
                            let data = self.read32(src_addr);
                            self.write32(dst_addr, data);
                        } else {
                            let data = self.read16(src_addr);
                            self.write16(dst_addr, data);
                        }
                        
                        self.tick(cycles);
                        *cycles += 1;

                        src_addr = match src_addr_control {
                            AddrControl::Increment => src_addr + addr_delta,
                            AddrControl::Decrement => src_addr - addr_delta,
                            _ => src_addr,
                        };

                        dst_addr = match dst_addr_control {
                            AddrControl::Increment | AddrControl::IncReload => dst_addr + addr_delta,
                            AddrControl::Decrement => dst_addr - addr_delta,
                            AddrControl::Fixed => dst_addr,
                        };
                    }

                    if !channels[ch].repeat || start_timing == StartTiming::Immediate {
                        self.dma_channels[ch].enable = false;
                    }

                    if channels[ch].dma_irq {
                        self.iff.set_dma(ch);
                    }

                    // self.ppu.vid_capture = false;
                    self.dma_in_progress[ch] = false;
                    self.dma_channels[ch].src = src_addr;
                    self.dma_channels[ch].dst = if dst_addr_control == AddrControl::IncReload { channels[ch].dst } else { dst_addr };
                }
            }
        }
    }

    // fn open_bus<const ARM: bool>(&mut self, pc: u32) -> u32 {
    //     if ARM {
    //         self.read32(pc + 8)
    //     } else {
    //         match pc >> 24 {
    //             0x02 | 0x05 | 0x06 | 0x08..=0x0D => {
    //                 ((self.read16(pc + 4) as u32) << 16) | (self.read16(pc + 4) as u32)
    //             }
    //             0x00 | 0x07 => {
    //                 if pc % 4 == 0 {
    //                     ((self.read16(pc + 6) as u32) << 16) | (self.read16(pc + 4) as u32)
    //                 } else {
    //                     ((self.read16(pc + 4) as u32) << 16) | (self.read16(pc + 2) as u32)
    //                 }
    //             }
    //             _ => {
    //                 if pc % 4 == 0 {
    //                     ((self.read16(pc + 2) as u32) << 16) | (self.read16(pc + 4) as u32)
    //                 } else {
    //                     ((self.read16(pc + 4) as u32) << 16) | (self.read16(pc + 2) as u32)
    //                 }
    //             }
    //         }
    //     }
    // }
}

impl Mcu for Bus {
    #[rustfmt::skip]
    fn read8(&mut self, address: u32) -> u8 {
        let a = match address >> 24 {
            0x00 if address < 0x4000 => self.bios[address as usize],
            0x02 => self.wram[address as usize % 0x0004_0000],
            0x03 => self.wram[(address as usize % 0x0000_8000) + 0x0004_0000],
            0x04 => match address - 0x0400_0000 {
                addr @ 0x0000..=0x0051 => self.ppu.read8(addr),
                addr @ 0x00B0..=0x00DF => self.dma_channels.read8(addr),
                addr @ 0x0100..=0x010F => self.timers.read8(addr),
                0x0130 => self.key_input.keyinput() as u8,
                0x0131 => (self.key_input.keyinput() >> 8) as u8,
                0x0200 => bits!(self.ie.0, 0..=7),
                0x0201 => bits!(self.ie.0, 8..=15),
                0x0202 => bits!(self.iff.0, 0..=7),
                0x0203 => bits!(self.iff.0, 8..=15),
                0x0208 => self.ime.enabled() as u8,
                0x0209 => bits!(self.ime.0, 8..=15),
                0x020A => bits!(self.ime.0, 16..=23),
                0x020B => bits!(self.ime.0, 24..=31),
                _ => 0x00,
            },
            0x05 => self.palette_ram[address as usize % 0x400],
            0x06 => self.vram[address as usize % 0x0001_8000],
            0x07 => self.oam[address as usize % 0x400],
            0x08..=0x0D => self.game_pak.rom[address as usize & 0x00FF_FFFF],
            0x0E..=0x0F => {
                // Flash ID workaround.
                if address == 0x0E00_0000 {
                    0x62
                } else if address == 0x0E00_0001 {
                    0x13
                } else {
                    self.game_pak.sram[address as usize % 0x0001_0000]   
                }
            }
            _ => 0,
        };

        if a == 0x6C {
            println!("possible 0x6C read detected");
            println!("-> {:X?}", self.cpu_r);
        }

        a
    }

    #[rustfmt::skip]
    fn write8(&mut self, address: u32, value: u8) {
        match address >> 24 {
            0x02 => self.wram[address as usize % 0x0004_0000] = value,
            0x03 => self.wram[(address as usize % 0x8000) + 0x0004_0000] = value,
            0x04 => match address - 0x0400_0000 {
                addr @ (0x0000..=0x004B | 0x0050..=0x0054) => self.ppu.write8(addr, value),
                addr @ 0x00B0..=0x00DF => self.dma_channels.write8(addr, value),
                addr @ 0x0100..=0x010F => self.timers.write8(addr, value),
                0x0200 => set_bits!(self.ie.0, 0..=7, value),
                0x0201 => set_bits!(self.ie.0, 8..=15, value),
                0x0202 => self.iff.set_iff((self.iff.iff() & !(value as u16)) & 0x3FFF),
                0x0203 => self.iff.set_iff((self.iff.iff() & !((value as u16) << 8)) & 0x3FFF),
                0x0208 => self.ime.set_enabled(value & 1 != 0),
                0x0209 => set_bits!(self.ime.0, 8..=15, value),
                0x020A => set_bits!(self.ime.0, 16..=23, value),
                0x020B => set_bits!(self.ime.0, 24..=31, value),
                0x0301 => {
                    self.halt = (value >> 7) == 0;
                    // if self.halt {
                        // println!("Entering HALT mode!");
                    // }
                },
                _ => {}
            },
            0x05 => self.palette_ram[address as usize % 0x400] = value,
            0x06 => {
                println!("{:X} | VRAM write to {address:X} with {value:X}, r3: {:X}, r4: {:X}", self.cpu_r[15], self.cpu_r[3], self.cpu_r[4]);
                self.vram[address as usize % 0x0001_8000] = value;
            },
            0x07 => self.oam[address as usize % 0x400] = value,
            0x0E..=0x0F => {
                if value != 0x00 {
                    print!("{}", value as char);
                }
                self.game_pak.sram[address as usize % 0x0001_0000] = value;
            },
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
