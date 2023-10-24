use proc_bitfield::bitfield;

use crate::{ppu::lcd::Ppu, arm::interpreter::arm7tdmi::{Cpsr, Arm7TDMI, Mode}};

use super::{Mcu, irq::{IME, IE, IF, Interrupt}};

#[derive(Default)]
pub struct Io {
    pub ppu: Ppu,
    pub key_input: KEYINPUT,
    pub ime: IME,
    pub ie: IE,
    pub iff: IF,
}

impl Io {
    // TODO: handle nicer
    pub fn handle_irq(&mut self, cpu: &mut Arm7TDMI) {
        if self.ime.enabled() && cpu.cpsr.irq() {
            for i_irq in 0..=13 {
                // let Ok(irq) = Interrupt::try_from(i_irq) else { return };
                if self.iff.iff() << i_irq != 0 && self.ie.ie() << i_irq != 0 {
                    cpu.regs[14] = cpu.regs[15] + 4;

                    cpu.spsr.set_cpsr(cpu.cpsr.cpsr());
                    cpu.cpsr.set_mode(Mode::Irq);
                    cpu.branch = true;

                    cpu.regs[15] = 0x18;
                }
            }
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

impl Default for KEYINPUT {
    fn default() -> Self {
        KEYINPUT(0xFF)
    }
}

impl Mcu for Io {
    fn read8(&mut self, address: u32) -> u8 {
        match address {
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
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        match address {
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
        }
    }
}
