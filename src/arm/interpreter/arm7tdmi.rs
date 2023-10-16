use std::collections::HashMap;

use crate::{fl, mmu::bus::Bus, mmu::Mcu};
use proc_bitfield::{bitfield, ConvRaw};

/// Saved Program Status Register as an alias for differentiation. Same structure as CPSR.
type Spsr = Cpsr;
/// Each mode has its own banked registers (mostly r13 and r14).
type BankedRegisters = (Spsr, [u32; 16]);

// Include the generated LUT at compile time.
include!(concat!(env!("OUT_DIR"), "/instructions.rs"));

#[derive(Default)]
pub struct Arm7TDMI {
    pub regs: [u32; 16],
    pub cpsr: Cpsr,

    pub bus: Bus,

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
    User = 0b10000,
    Fiq = 0b10001,
    Irq = 0b10010,
    Supervisor = 0b10011,
    Abort = 0b10111,
    Undefined = 0b11011,
}

bitfield! {
    /// **CPSR**: Current Program Status Register.
    ///
    /// Unused here: bits 8-9 arm11 only, 10-23 & 25-26 reserved, 24 unnecessary, 27 armv5 upwards.
    #[derive(Clone, Copy, Default)]
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
    // TODO.
    pub fn setup_registers(skip_crt0: bool) -> Self {
        let mut regs = [0; 16];

        // temp crt0 skip
        if skip_crt0 {
            regs[2] = 0x0200_0000;
            regs[3] = 0x0800_02D8;
            regs[4] = 0x0200_0000;
            regs[13] = 0x0300_7F00;
            regs[14] = 0x0800_0187;
            regs[15] = 0x0800_02D8;
        } else {
            regs[13] = 0x0300_7F00;
            regs[15] = 0x0800_0000;
        }

        Self {
            regs,
            cpsr: Cpsr(0x6000_001F),
            ..Default::default()
        }
    }

    pub fn cycle(&mut self) {
        let opcode = self.bus.read32(self.regs[15]);

        let cond = (opcode & 0xF000_0000) >> 28;
        let op_index = ((opcode & 0x0FF0_0000) >> 16) | ((opcode & 0x00F0) >> 4);

        if self.cond(cond as u8) {
            // println!("{:X?}\n", self.regs);
            match self.cpsr.state() {
                State::Arm => ARM_INSTRUCTIONS[op_index as usize](self, opcode),
                State::Thumb => todo!(),
            }
        }

        self.regs[15] += 4;
    }

    /// If `I` is false, operand 2 is a register and gets shifted.
    /// Otherwise, it is an unsigned 8 bit immediate value.
    pub fn barrel_shifter<const I: bool>(&self, op: u16) -> (u32, bool) {
        if I {
            (
                ((op & 0xFF) as u32).rotate_right(((op as u32 & 0x0F00) >> 8) * 2),
                false,
            )
        } else {
            let mut rm = if (op as usize & 0xF) == 15 {
                self.regs[op as usize & 0xF] + 8
            } else {
                self.regs[op as usize & 0xF]
            };

            let shift_type = (op & 0x0060) >> 5;
            let amount = if op & (1 << 4) != 0 {
                if (op as usize & 0xF) == 15 {
                    rm += 4
                };
                self.regs[(op as usize & 0x0F00) >> 8] & 0xFF
            } else {
                (op as u32 & 0x0F80) >> 7
            };

            // `reg` parameter as there is different behavior depending on
            // if the amount is an immediate or register-specified.
            match shift_type {
                0b00 => self.lsl(rm, amount, op & (1 << 4) != 0),
                0b01 => self.lsr(rm, amount, op & (1 << 4) != 0),
                0b10 => self.asr(rm, amount, op & (1 << 4) != 0),
                0b11 => self.ror(rm, amount, op & (1 << 4) != 0),
                _ => unreachable!(),
            }
        }
    }

    pub fn cond(&self, cond: u8) -> bool {
        match cond {
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

    pub fn data_processing<const I: bool, const S: bool>(&mut self, opcode: u32) {
        let rd = (opcode as usize & 0xF000) >> 12;
        let rn = self.regs[(opcode as usize & 0x000F_0000) >> 16];
        let (op2, carry_out) = self.barrel_shifter::<I>(opcode as u16);

        // Bits 21-24 specify the actual opcode.
        let operation = (opcode & 0x01E0_0000) >> 21;
        // Check if TST, TEQ, CMP, CMN.
        let mut is_intmd = false;

        #[rustfmt::skip]
        let result = match operation {
            0b0000 => rn & op2,
            0b0001 => rn ^ op2,
            0b0010 => fl!(rn, op2, -, self, cpsr),
            0b0011 => fl!(op2, rn, -, self, cpsr),
            0b0100 => fl!(rn, op2, +, self, cpsr),
            0b0101 => fl!(rn, op2 + self.cpsr.c() as u32, +, self, cpsr),
            0b0110 => fl!(rn, op2, self.cpsr.c() as u32 - 1, -, self, cpsr),
            0b0111 => fl!(op2, rn, self.cpsr.c() as u32 - 1, -, self, cpsr),
            0b1000 => {is_intmd = true; rn & op2},
            0b1001 => {is_intmd = true; rn ^ op2},
            0b1010 => {is_intmd = true; fl!(rn, op2, -, self, cpsr)},
            0b1011 => {is_intmd = true; fl!(rn, op2, +, self, cpsr)},
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
                self.cpsr.set_z(result == 0);
                // Set N flag to bit 31 of result.
                self.cpsr.set_n(result & (1 << 31) != 0);

                // Logical operations set Carry from barrel shifter and leave V unaffected.
                // Arithmetic operations handle their flags in the fl! macro.
                if matches!(
                    operation,
                    0b0000 | 0b0001 | 0b1000 | 0b1001 | 0b1100 | 0b1101 | 0b1110 | 0b1111
                ) {
                    self.cpsr.set_c(carry_out);
                }
            }
        }

        // FIXME: temporary check for mov r15, r1-r13.
        if !is_intmd {
            self.regs[rd] = if rd == 15 && (opcode & 0xF) != 14 {
                result - 4
            } else {
                result
            };
        }
    }

    /// MUL and MLA. (check for r15 and rd != rm?)
    pub fn multiply<const S: bool>(&mut self, opcode: u32) {
        let acc = (opcode & (1 << 21)) != 0;

        let rd = (opcode as usize & 0x000F_0000) >> 16;
        let rm = self.regs[opcode as usize & 0xF];
        let rs = self.regs[(opcode as usize & 0x0F00) >> 8];
        let rn = self.regs[(opcode as usize & 0xF000) >> 12];

        assert_ne!(rd, 15);
        assert_ne!(rd, opcode as usize & 0xF);

        self.regs[rd] = rm * rs + (rn * acc as u32);

        if S {
            self.cpsr.set_n(self.regs[rd] & (1 << 31) != 0);
            self.cpsr.set_z(self.regs[rd] == 0)
        }
    }

    /// MULL and MLAL. (check for r15 and rd != rm?)
    pub fn multiply_long<const S: bool>(&mut self, opcode: u32) {
        let acc = (opcode & (1 << 21)) != 0;
        let signed = (opcode & (1 << 22)) != 0;

        let rd_hi = (opcode as usize & 0x000F_0000) >> 16;
        let rd_lo = (opcode as usize & 0xF000) >> 12;
        let rs = self.regs[(opcode as usize & 0x0F00) >> 8];
        let rm = self.regs[opcode as usize & 0xF];

        let combined_rd = ((self.regs[rd_hi] as u64) << 32) | self.regs[rd_lo] as u64;

        // TODO: not needed?
        let res = match signed {
            false => rm as u64 * rs as u64 + (combined_rd * acc as u64),
            true => (rm as i64 * rs as i64 + (combined_rd as i64 * acc as i64)) as u64,
        };

        self.regs[rd_hi] = ((res & 0xFFFF_0000) >> 32) as u32;
        self.regs[rd_lo] = res as u32;

        if S {
            self.cpsr.set_n(res & (1 << 63) != 0);
            self.cpsr.set_z(res == 0)
        }
    }

    /// Single Data Swap (SWP).
    pub fn swap<const B: bool>(&mut self, opcode: u32) {
        let rd = (opcode as usize & 0xF000) >> 12;
        let rn = self.regs[(opcode as usize & 0x000F_0000) >> 16];
        let rm = self.regs[opcode as usize & 0xF];

        match B {
            false => {
                let (aligned_address, data_ror) = if rn % 4 != 0 {
                    (rn & !3, (rn & 3) * 8)
                } else {
                    (rn, 0)
                };

                let swp_content = self.bus.read32(aligned_address);
                self.bus.write32(rn, rm);
                self.regs[rd] = swp_content.rotate_right(data_ror);
            }
            true => {
                let swp_content = self.bus.read8(rn);
                self.bus.write32(rn, rm);
                self.regs[rd] = swp_content as u32;
            }
        }
    }

    /// Branch and Exchange.
    pub fn bx(&mut self, opcode: u32) {
        let rn = self.regs[opcode as usize & 0xF];

        self.regs[15] = rn;

        // Bit 0 of Rn decides decoding of subsequent instructions.
        if rn & 1 == 0 {
            self.cpsr.set_state(State::Arm);
        } else {
            self.cpsr.set_state(State::Thumb);
        }
    }

    /// Branch and Link.
    pub fn bl(&mut self, opcode: u32) {
        let ioffset = (opcode & 0x00FF_FFFF) << 2;
        let link = opcode & (1 << 24) != 0;

        let ioffset = if opcode & (1 << 23) != 0 {
            0xFF00_0000 | ioffset
        } else {
            ioffset & 0x00FF_FFFF
        } as i32;

        // TODO: r15 offset adjustment (+4 in cycle or here???)
        if link {
            self.regs[14] = self.regs[15];
        }

        self.regs[15] = self.regs[15].wrapping_add_signed(ioffset + 4);
    }

    /// PSR Transfer. Transfer contents of CPSR/SPSR between registers.
    pub fn psr_transfer<const I: bool>(&mut self, opcode: u32) {
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

    /// Software Interrupt.
    pub fn swi(&mut self, _opcode: u32) {
        self.swap_regs(Mode::Supervisor);
        self.cpsr.set_mode(Mode::Supervisor);

        self.spsr = self.cpsr;
        self.regs[14] = self.regs[15];
        self.regs[15] = 0x08;
    }

    /// LDR and STR.
    pub fn single_data_transfer<
        const I: bool,
        const P: bool,
        const U: bool,
        const B: bool,
        const W: bool,
        const L: bool,
    >(
        &mut self,
        opcode: u32,
    ) {
        let rn = (opcode as usize & 0x000F_0000) >> 16;
        let rd = (opcode as usize & 0xF000) >> 12;
        let offset = if !I {
            opcode & 0x0FFF
        } else {
            self.barrel_shifter::<false>(opcode as u16).0
        };

        let base_with_offset = if U {
            self.regs[rn] + offset
        } else {
            self.regs[rn] - offset
        };

        let mut address = if P { base_with_offset } else { self.regs[rn] };
        #[rustfmt::skip]
        if rn == 15 { address += 8 };

        // Load from memory if L, else store register into memory.
        if L {
            // Force align address.
            let (aligned_address, data_ror) = if !B && address % 4 != 0 {
                (address & !3, (address & 3) * 8)
            } else {
                (address, 0)
            };

            let val = if B {
                self.bus.read8(aligned_address) as u32
            } else {
                self.bus.read32(aligned_address)
            };

            self.regs[rd] = val.rotate_right(data_ror);
        } else {
            let aligned_address = if !B && address % 4 != 0 {
                address & !3
            } else {
                address
            };

            if B {
                self.bus.write8(aligned_address, self.regs[rd] as u8);
            } else {
                self.bus.write32(aligned_address, self.regs[rd]);
            }
        }

        if W || !P {
            self.regs[rn] = base_with_offset;
        }
    }

    /// LDRH/STRH and LDRSB/LDRSH
    pub fn hw_signed_data_transfer<
        const I: bool,
        const P: bool,
        const U: bool,
        const W: bool,
        const L: bool,
        const S: bool,
        const H: bool,
    >(
        &mut self,
        opcode: u32,
    ) {
        let rn = (opcode as usize & 0x000F_0000) >> 16;
        let rd = (opcode as usize & 0xF000) >> 12;
        let offset = if I {
            ((opcode & 0xF00) >> 4) | (opcode & 0xF)
        } else {
            self.regs[opcode as usize & 0xF]
        };

        let base_with_offset = if U {
            self.regs[rn] + offset
        } else {
            self.regs[rn] - offset
        };

        let address = if P { base_with_offset } else { self.regs[rn] };

        // Load from memory if L, else store register into memory.
        if L {
            if !S {
                self.regs[rd] = self.bus.read16(address) as u32;
            } else {
                self.regs[rd] = match H {
                    false => {
                        let sb = self.bus.read8(address);
                        sb as i32 as u32
                    }
                    true => {
                        let shw = self.bus.read16(address);
                        shw as i32 as u32
                    }
                }
            }
        } else {
            assert_eq!(S, false);
            self.bus.write16(address, self.regs[rd] as u16);
        }

        if W || !P {
            self.regs[rn] = base_with_offset;
        }
    }

    /// LDM/STM (todo: psr bit)
    pub fn block_data_transfer<
        const P: bool,
        const U: bool,
        const S: bool,
        const W: bool,
        const L: bool,
    >(
        &mut self,
        opcode: u32,
    ) {
        let rn = (opcode as usize & 0x000F_0000) >> 16;
        let mut reg_list = (0..16)
            .filter(|i| (opcode as u16) & (1 << i) != 0)
            .collect::<Vec<_>>();

        let mut address = self.regs[rn];
        if !U {
            reg_list.reverse();
        }

        for r in &reg_list {
            if P {
                // Pre-{inc, dec}rement addressing.
                address = if U { address + 4 } else { address - 4 };

                if L {
                    self.regs[*r] = self.bus.read32(address);
                } else {
                    self.bus.write32(address, self.regs[*r]);
                }
            } else {
                if L {
                    self.regs[*r] = self.bus.read32(address);
                } else {
                    self.bus.write32(address, self.regs[*r]);
                }

                // Post-{inc, dec}rement addressing.
                address = if U { address + 4 } else { address - 4 };
            }
        }

        // TODO: check for base register in register list
        if W || !P {
            self.regs[rn] = address;
        }
    }

    // Test for LUT.
    pub fn dummy(&mut self, _opcode: u32) {
        panic!("shouldn't be called!")
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
    fn lsl(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
        match reg {
            false => (rm << amount, rm & (1 << (32 - amount + 1)) != 0),
            true => {
                if amount == 0 {
                    (rm, self.cpsr.c())
                } else if amount < 32 {
                    (rm << amount, rm & (1 << (32 - amount + 1)) != 0)
                } else {
                    (0, (rm & 1) != 0)
                }
            }
        }
    }

    /// Logical shift right, returns result and carry out.
    #[inline(always)]
    fn lsr(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
        match reg {
            false => {
                if amount == 0 {
                    (0, rm & (1 << 31) != 0)
                } else {
                    (rm >> amount, rm & (1 << (amount - 1)) != 0)
                }
            }
            true => {
                if amount == 0 {
                    (rm, self.cpsr.c())
                } else if amount < 32 {
                    (rm >> amount, rm & (1 << (amount - 1)) != 0)
                } else {
                    (0, false)
                }
            }
        }
    }

    /// Arithmetic shift right, returns result and carry out.
    #[inline(always)]
    fn asr(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
        if reg && amount == 0 {
            return (rm, self.cpsr.c());
        }

        let bit31 = rm & (1 << 31);
        let carry = rm & (1 << (amount - 1)) != 0;

        let mut rm = rm >> amount;
        for i in 0..amount {
            rm |= bit31 >> i;
        }

        if amount == 0 || amount >= 32 {
            ((bit31 >> 31) * 0xFFFF_FFFF, bit31 != 0)
        } else {
            (rm, carry)
        }
    }

    /// Rotate right, returns result and carry out.
    #[inline(always)]
    fn ror(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
        if amount == 0 {
            if reg {
                return (rm, self.cpsr.c());
            } else {
                return ((self.cpsr.c() as u32) << 31 | (rm >> 1), (rm & 1) != 0);
            }
        }

        (rm.rotate_right(amount), rm & (1 << (amount - 1)) != 0)
    }
}
