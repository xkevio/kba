use std::collections::HashMap;

use crate::{mmu::bus::Bus, mmu::Mcu, ov};
use proc_bitfield::{bitfield, ConvRaw};

/// Saved Program Status Register as an alias for differentiation. Same structure as CPSR.
type Spsr = Cpsr;
/// Each mode has its own banked registers (mostly r13 and r14).
type BankedRegisters = (Spsr, [u32; 16]);

pub struct Arm7TDMI {
    pub regs: [u32; 16],
    pub cpsr: Cpsr,

    spsr: Spsr,
    banked_regs: HashMap<Mode, BankedRegisters>,
}

pub enum State {
    Arm,
    Thumb,
}

/// Each mode has own PSR (SPSR) and some registers.
/// See `banked_regs` in `Arm7TDMI`.
#[derive(ConvRaw, Hash, PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    User = 0b0000,
    Fiq = 0b0001,
    Irq = 0b0010,
    Supervisor = 0b0011,
    Abort = 0b0111,
    Undefined = 0b1011,
}

bitfield! {
    /// **CPSR**: Current Program Status Register.
    ///
    /// Unused here: bits 8-9 arm11 only, 10-23 & 25-26 reserved, 24 unnecessary, 27 armv5 upwards.
    #[derive(Clone, Copy)]
    pub struct Cpsr(pub u32) {
        pub cpsr: u32 @ ..,
        /// Mode bits (fiq, irq, svc, user...)
        pub mode: u8 [try Mode] @ 0..=4,
        /// ARM (0) or THUMB (1) - T bit
        pub state: bool [State] @ 5,
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

impl From<bool> for State {
    fn from(value: bool) -> Self {
        match value {
            false => Self::Arm,
            true => Self::Thumb,
        }
    }
}

impl From<State> for bool {
    fn from(value: State) -> Self {
        match value {
            State::Arm => false,
            State::Thumb => true,
        }
    }
}

impl Arm7TDMI {
    /// If `I` is false, operand 2 is a register and gets shifted.
    /// Otherwise, it is an unsigned 8 bit immediate value.
    pub fn barrel_shifter<const I: bool>(&self, op: u16) -> (u32, bool) {
        if I {
            (
                ((op & 0xFF) as u32).rotate_right((op as u32 & 0x0F00) * 2),
                false,
            )
        } else {
            let rm = self.regs[op as usize & 0xF];
            let shift_type = (op & 0x0060) >> 5;
            let amount = if op & (1 << 3) != 0 {
                self.regs[(op as usize & 0x0F00) >> 8]
            } else {
                (op as u32 & 0x0F80) >> 7
            };

            match shift_type {
                0b00 => self.lsl(rm, amount),
                0b01 => self.lsr(rm, amount),
                0b10 => self.asr(rm, amount),
                0b11 => self.ror(rm, amount),
                _ => unreachable!(),
            }
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
        let (op2, carry_out) = self.barrel_shifter::<I>(opcode as u16);

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

            if S {
                if rd == 15 {
                    self.cpsr
                        .set_cpsr(self.banked_regs[&self.cpsr.mode().unwrap()].0.cpsr());
                } else {
                    // Set Zero flag iff result is all zeros.
                    if result == 0 {
                        self.cpsr.set_z(true);
                    }
                    // Set N flag to bit 31 of result.
                    self.cpsr.set_n(result & (1 << 31) != 0);

                    // Logical operations set Carry from barrel shifter.
                    if matches!(
                        operation,
                        0b0000 | 0b0001 | 0b1000 | 0b1001 | 0b1100 | 0b1101 | 0b1110 | 0b1111
                    ) {
                        self.cpsr.set_c(carry_out);
                    } else {
                        // TODO: set c to carry out of bit31 in ALU.
                    }
                }
            }

            if !is_intmd {
                self.regs[rd] = result;
            }
        }
    }

    /// MUL and MLA. (check for r15 and rd != rm?)
    pub fn multiply<const COND: u8, const S: bool>(&mut self, opcode: u32) {
        let acc = (opcode & (1 << 21)) != 0;

        let rd = (opcode as usize & 0x000F_0000) >> 16;
        let rm = self.regs[opcode as usize & 0xF];
        let rs = self.regs[(opcode as usize & 0x0F00) >> 8];
        let rn = self.regs[(opcode as usize & 0xF000) >> 12];

        assert_ne!(rd, 15);
        assert_ne!(rd, opcode as usize & 0xF);

        if self.cond::<COND>() {
            self.regs[rd] = rm * rs + (rn * acc as u32);

            if S {
                self.cpsr.set_n(self.regs[rd] & (1 << 31) != 0);
                if self.regs[rd] == 0 {
                    self.cpsr.set_z(true)
                };
            }
        }
    }

    /// MULL and MLAL. (check for r15 and rd != rm?)
    pub fn multiply_long<const COND: u8, const S: bool>(&mut self, opcode: u32) {
        let acc = (opcode & (1 << 21)) != 0;
        let signed = (opcode & (1 << 22)) != 0;

        let rd_hi = (opcode as usize & 0x000F_0000) >> 16;
        let rd_lo = (opcode as usize & 0xF000) >> 12;
        let rs = self.regs[(opcode as usize & 0x0F00) >> 8];
        let rm = self.regs[opcode as usize & 0xF];

        let combined_rd = ((self.regs[rd_hi] as u64) << 32) | self.regs[rd_lo] as u64;

        if self.cond::<COND>() {
            // TODO: not needed?
            let res = match signed {
                false => rm as u64 * rs as u64 + (combined_rd * acc as u64),
                true => (rm as i64 * rs as i64 + (combined_rd as i64 * acc as i64)) as u64,
            };

            self.regs[rd_hi] = ((res & 0xFFFF_0000) >> 32) as u32;
            self.regs[rd_lo] = res as u32;

            if S {
                self.cpsr.set_n(res & (1 << 63) != 0);
                if res == 0 {
                    self.cpsr.set_z(true)
                };
            }
        }
    }

    /// Single Data Swap (SWP). todo: bus parameter change.
    pub fn swap<const COND: u8>(&mut self, opcode: u32, bus: &mut Bus) {
        let bw_bit = (opcode & (1 << 22)) >> 22;

        let rd = (opcode as usize & 0xF000) >> 12;
        let rn = self.regs[(opcode as usize & 0x000F_0000) >> 16];
        let rm = self.regs[opcode as usize & 0xF];

        if self.cond::<COND>() {
            match bw_bit {
                0 => {
                    let swp_content = bus.read::<u32>(rn);
                    bus.write(rn, rm);
                    self.regs[rd] = swp_content;
                }
                1 => {
                    let swp_content = bus.read::<u8>(rn);
                    bus.write(rn, rm);
                    self.regs[rd] = swp_content as u32;
                }
                _ => unreachable!(),
            }
        }
    }

    /// Branch and Exchange.
    pub fn bx<const COND: u8>(&mut self, opcode: u32) {
        let rn = self.regs[opcode as usize & 0xF];

        if self.cond::<COND>() {
            self.regs[15] = rn;

            // Bit 0 of Rn decides decoding of subsequent instructions.
            if rn & 1 == 0 {
                self.cpsr.set_state(State::Arm);
            } else {
                self.cpsr.set_state(State::Thumb);
            }
        }
    }

    /// Branch and Link.
    pub fn bl<const COND: u8>(&mut self, opcode: u32) {
        let offset = ((opcode & 0x00FF_FFFF) << 2) as i32;
        let link = opcode & (1 << 24) != 0;

        // TODO: r15 offset adjustment
        if self.cond::<COND>() {
            if link {
                self.regs[14] = self.regs[15] + 4;
            }

            self.regs[15] = self.regs[15].wrapping_add_signed(offset + 8);
        }
    }

    /// PSR Transfer. Transfer contents of CPSR/SPSR between registers.
    pub fn psr_transfer<const COND: u8, const I: bool>(&mut self, opcode: u32) {
        if self.cond::<COND>() {
            let mut source_psr = if opcode & (1 << 22) != 0 {
                self.spsr
            } else {
                self.cpsr
            };

            // MRS (transfer PSR contents to register)
            if (opcode & 0x000F_0000) >> 16 == 0b1111 {
                let rd = (opcode as usize & 0xF000) >> 12;
                self.regs[rd] = source_psr.cpsr();
            }
            // MSR (transfer register contents to PSR)
            else if (opcode & 0x000F_0000) >> 16 == 0b1001 {
                let rm = self.regs[opcode as usize & 0xF];
                source_psr.set_cpsr(rm);
            }
            // MSR (transfer register contents or immediate to PSR flag bits)
            else {
                if !I {
                    let rm = self.regs[opcode as usize & 0xF];
                    source_psr.set_cpsr((rm & 0xF000_0000) | (source_psr.cpsr() & 0x0FFF_FFFF));
                } else {
                    let (imm, _) = self.barrel_shifter::<I>(opcode as u16);
                    source_psr.set_cpsr((imm & 0xF000_0000) | (source_psr.cpsr() & 0x0FFF_FFFF));
                }
            }

            if opcode & (1 << 22) != 0 {
                self.spsr = source_psr;
            } else {
                self.cpsr = source_psr;
            }
        }
    }

    /// Software Interrupt.
    pub fn swi<const COND: u8>(&mut self, _opcode: u32) {
        if self.cond::<COND>() {
            self.swap_regs(Mode::Supervisor);
            self.cpsr.set_mode(Mode::Supervisor);

            self.spsr = self.cpsr;
            self.regs[14] = self.regs[15];
            self.regs[15] = 0x08;
        }
    }

    /// LDR and STR.
    pub fn single_data_transfer<
        const COND: u8,
        const I: bool,
        const P: bool,
        const U: bool,
        const B: bool,
        const W: bool,
        const L: bool,
    >(
        &mut self,
        opcode: u32,
        bus: &mut Bus,
    ) {
        if self.cond::<COND>() {
            let rn = (opcode as usize & 0x000F_0000) >> 16;
            let rd = (opcode as usize & 0xF000) >> 12;
            let offset = opcode & 0x0FFF; // todo shift.

            let base_with_offset = if U {
                self.regs[rn] + offset
            } else {
                self.regs[rn] - offset
            };

            let address = if P { base_with_offset } else { self.regs[rn] };

            // Load from memory if L, else store register into memory.
            if L {
                let val = if B {
                    bus.read::<u8>(address) as u32
                } else {
                    bus.read::<u32>(address)
                };

                if W {
                    self.regs[rn] = address;
                }

                self.regs[rd] = val;
            } else {
                todo!()
            }
        }
    }

    /// Swap banked registers on mode change. Call before changing mode in CPSR.
    fn swap_regs(&mut self, new_mode: Mode) {
        let (spsr_mode, bank_regs) = self.banked_regs[&new_mode];
        let Ok(current_mode) = self.cpsr.mode() else {
            return;
        };

        self.banked_regs
            .insert(current_mode, (self.spsr, self.regs));

        self.spsr = spsr_mode;
        self.regs = bank_regs;
    }

    /// Logical shift left, returns result and carry out.
    #[inline(always)]
    fn lsl(&self, rm: u32, amount: u32) -> (u32, bool) {
        (rm << amount, rm & (32 - amount) != 0)
    }

    /// Logical shift right, returns result and carry out.
    #[inline(always)]
    fn lsr(&self, rm: u32, amount: u32) -> (u32, bool) {
        (rm >> amount, rm & amount != 0)
    }

    /// Arithmetic shift right, returns result and carry out.
    #[inline(always)]
    fn asr(&self, rm: u32, amount: u32) -> (u32, bool) {
        let bit31 = rm & (1 << 31);
        let carry = rm & amount != 0;

        let mut rm = rm >> amount;
        for i in 0..amount {
            rm |= bit31 >> i;
        }

        (rm, carry)
    }

    /// Rotate right, returns result and carry out.
    #[inline(always)]
    fn ror(&self, rm: u32, amount: u32) -> (u32, bool) {
        (rm.rotate_right(amount), rm & amount != 0)
    }
}
