use proc_bitfield::bitfield;

use crate::ov;

pub struct Arm7TDMI {
    pub regs: [u32; 16],
    pub cpsr: Cpsr,
}

pub enum State {
    Arm,
    Thumb,
}

impl From<bool> for State {
    fn from(value: bool) -> Self {
        match value {
            false => Self::Arm,
            true => Self::Thumb,
        }
    }
}

bitfield! {
    /// **CPSR**: Current Program Status Register.
    ///
    /// Unused here: bits 8-9 arm11 only, 10-23 & 25-26 reserved, 24 unnecessary, 27 armv5 upwards.
    pub struct Cpsr(pub u32) {
        pub cpsr: u32 @ ..,
        /// Mode bits (fiq, irq, svc, user...)
        pub mode: u8 @ 0..=4,
        /// ARM (0) or THUMB (1)
        pub state: bool [get State] @ 5,
        pub fiq: bool @ 6,
        pub irq: bool @ 7,
        /// Overflow flag
        pub v: bool @ 28,
        /// Carry flag
        pub c: bool @ 29,
        /// Zero flag
        pub z: bool @ 30,
        /// Sign flag
        pub n: bool @ 31,
    }
}

impl Arm7TDMI {
    /// If I is false, operand 2 is a register and gets shifted.
    /// Otherwise, it is an unsigned 8 bit immediate value.
    pub fn barrel_shifter<const I: bool>(&self, op: u16) -> u32 {
        if I {
            ((op & 0xFF) as u32).rotate_right((op as u32 & 0x0F00) * 2)
        } else {
            let rm = self.regs[op as usize & 0xF];
            let shift = op & 0x0FF0;
            todo!()
        }
    }

    pub fn cond<const COND: u8>(&self) -> bool {
        match COND {
            0b0000 => self.cpsr.z(),
            0b0001 => !self.cpsr.z(),
            0b0010 => self.cpsr.c(),
            0b0011 => !self.cpsr.c(),
            0b0100 => self.cpsr.n(),
            0b0101 => !self.cpsr.n(),
            0b0110 => self.cpsr.v(),
            0b0111 => !self.cpsr.v(),
            0b1000 => self.cpsr.c() && !self.cpsr.z(),
            0b1001 => self.cpsr.c() && self.cpsr.z(),
            0b1010 => self.cpsr.n() == self.cpsr.v(),
            0b1011 => self.cpsr.n() != self.cpsr.v(),
            0b1100 => !self.cpsr.z() && (self.cpsr.n() == self.cpsr.v()),
            0b1101 => self.cpsr.z() || (self.cpsr.n() != self.cpsr.v()),
            0b1110 => true,
            _ => unreachable!(),
        }
    }

    pub fn data_processing<const COND: u8, const I: bool, const S: bool>(&mut self, opcode: u32) {
        let rd = (opcode as usize & 0xF000) >> 12;
        let rn = self.regs[(opcode as usize & 0x000F_0000) >> 16];
        let op2 = self.barrel_shifter::<I>(opcode as u16);

        // Bits 21-24 specify the actual opcode.
        let operation = (opcode & 0x01E0_0000) >> 21;
        // Check if TST, TEQ, CMP, CMN.
        let mut is_intmd = false;

        // TODO: carry out from barrel shifter
        if self.cond::<COND>() {
            #[rustfmt::skip]
            let result = match operation {
                0b0000 => rn & op2,
                0b0001 => rn ^ op2,
                0b0010 => ov!(rn.overflowing_sub(op2), opcode, self),
                0b0011 => ov!(op2.overflowing_sub(rn), opcode, self),
                0b0100 => ov!(rn.overflowing_add(op2), opcode, self),
                0b0101 => ov!(rn.overflowing_add(op2 + self.cpsr.c() as u32), opcode, self),
                0b0110 => ov!(rn.overflowing_sub(op2 + self.cpsr.c() as u32 - 1), opcode, self),
                0b0111 => ov!(op2.overflowing_sub(rn + self.cpsr.c() as u32 - 1), opcode, self),
                0b1000 => {is_intmd = true; rn & op2},
                0b1001 => {is_intmd = true; rn ^ op2},
                0b1010 => {is_intmd = true; ov!(rn.overflowing_sub(op2), opcode, self)},
                0b1011 => {is_intmd = true; ov!(rn.overflowing_add(op2), opcode, self)},
                0b1100 => rn | op2,
                0b1101 => op2,
                0b1110 => rn & !(op2),
                0b1111 => !op2,
                _ => unreachable!()
            };

            // If S-bit is set and if rd != r15.
            if S && ((opcode as usize & 0xF000) >> 12) != 15 {
                // Set Zero flag iff result is all zeros.
                if result == 0 {
                    self.cpsr.set_z(true);
                }
                // Set N flag to bit 31 of result.
                self.cpsr.set_n(result & (1 << 31) != 0);

                // Logical operations.
                if matches!(
                    operation,
                    0b0000 | 0b0001 | 0b1000 | 0b1001 | 0b1100 | 0b1101 | 0b1110 | 0b1111
                ) {
                    // TODO: set c to carry out of barrel shifter.
                    // Arithmetic operations.
                } else {
                    // TODO: set c to carry out of bit31 in ALU.
                }
            }

            if !is_intmd {
                self.regs[rd] = result;
            }
        }
    }
}
