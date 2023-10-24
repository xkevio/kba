use std::collections::HashMap;

use crate::{
    arm::arr_with,
    fl,
    mmu::{bus::Bus, game_pak::GamePak, Mcu},
};
use proc_bitfield::{bitfield, ConvRaw};

/// Saved Program Status Register as an alias for differentiation. Same structure as CPSR.
type Spsr = Cpsr;
/// Each mode has its own banked registers (mostly r13 and r14).
type BankedRegisters = (Spsr, [u32; 16]);

// Include the generated LUT at compile time.
include!(concat!(env!("OUT_DIR"), "/arm_instructions.rs"));
include!(concat!(env!("OUT_DIR"), "/thumb_instructions.rs"));

#[derive(Default)]
pub struct Arm7TDMI {
    /// 16 registers, most GPR, r14 = LR, r15 = PC.
    pub regs: [u32; 16],
    /// Current Program Status Register.
    pub cpsr: Cpsr,

    /// The memory bus, owned by the CPU for now.
    pub bus: Bus,

    /// Saved Program Status Register for all modes but User.
    spsr: Spsr,
    /// The other banked registers of the other modes.
    banked_regs: HashMap<Mode, BankedRegisters>,

    /// If the prev. instruction directly **set** r15.
    pub(super) branch: bool,
}

#[derive(PartialEq)]
pub enum State {
    Arm,
    Thumb,
}

/// Each mode has own PSR (SPSR) and some registers.
/// See `banked_regs` in `Arm7TDMI`.
#[derive(ConvRaw, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Mode {
    User = 0b10000,
    Fiq = 0b10001,
    Irq = 0b10010,
    Supervisor = 0b10011,
    Abort = 0b10111,
    Undefined = 0b11011,
    System = 0b11111,
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
    /// Initialize SP and PC to the correct values.
    pub fn new(rom: &[u8]) -> Self {
        let mut regs = [0; 16];

        // Initialize GamePak memory.
        let bus = Bus {
            game_pak: GamePak {
                rom: rom.to_vec(),
                ..Default::default()
            },
            ..Default::default()
        };

        // Skip BIOS.
        regs[13] = 0x0300_7F00;
        regs[15] = 0x0800_0000;

        // Set other modes r13 (SP) and SPSR.
        let banked_regs = HashMap::from([
            (Mode::System, (Cpsr(0), arr_with(13, 0x0300_7F00))),
            (Mode::Irq, (Cpsr(0), arr_with(13, 0x0300_7FA0))),
            (Mode::Supervisor, (Cpsr(0), arr_with(13, 0x0300_7FE0))),
            (Mode::Fiq, (Cpsr(0), arr_with(13, 0x0300_7FF0))),
            (Mode::Abort, (Cpsr(0), arr_with(13, 0x0300_7FF0))),
            (Mode::Undefined, (Cpsr(0), arr_with(13, 0x0300_7FF0))),
        ]);

        Self {
            regs,
            cpsr: Cpsr(0x6000_001F),
            bus,
            banked_regs,
            ..Default::default()
        }
    }

    /// Cycle through an instruction with 1 CPI.
    pub fn cycle(&mut self) {
        match self.cpsr.state() {
            State::Arm => {
                let opcode = self.bus.read32(self.regs[15]);

                let cond = (opcode >> 28) & 0xF;
                let op_index = ((opcode & 0x0FF0_0000) >> 16) | ((opcode & 0x00F0) >> 4);

                if self.cond(cond as u8) {
                    ARM_INSTRUCTIONS[op_index as usize](self, opcode);
                }
            }
            State::Thumb => {
                let opcode = self.bus.read16(self.regs[15]);
                THUMB_INSTRUCTIONS[(opcode >> 8) as usize](self, opcode);
            }
        }

        self.regs[15] += match self.cpsr.state() {
            State::Arm if !self.branch => 4,
            State::Thumb if !self.branch => 2,
            _ => 0,
        };

        self.branch = false;
    }

    // ARM INSTRUCTIONS IMPLEMENTATION & SHIFTER.

    /// If `I` is false, operand 2 is a register and gets shifted.
    /// Otherwise, it is an unsigned 8 bit immediate value.
    pub fn barrel_shifter<const I: bool>(&self, op: u16) -> (u32, bool) {
        if I {
            let ror = (op as u32 >> 8) & 0xF;
            let res = (op as u32 & 0xFF).rotate_right(ror * 2);
            let c = if ror == 0 {
                self.cpsr.c()
            } else {
                (res >> 31) != 0
            };
            (res, c)
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
            0b1001 => !self.cpsr.c() || self.cpsr.z(),
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
        // If operand is PC, add 8.
        let rn = if (opcode & 0x000F_0000) >> 16 == 15 {
            if !I && (opcode & (1 << 4)) != 0 {
                rn + 12
            } else {
                rn + 8
            }
        } else {
            rn
        };

        #[rustfmt::skip]
        let result = match operation {
            0b0000 => rn & op2,
            0b0001 => rn ^ op2,
            0b0010 => fl!(rn, op2, -, self, cpsr, S),
            0b0011 => fl!(op2, rn, -, self, cpsr, S),
            0b0100 => fl!(rn, op2, +, self, cpsr, S),
            0b0101 => fl!(rn, op2 + self.cpsr.c() as u32, +, self, cpsr, S),
            0b0110 => fl!(rn, op2, !self.cpsr.c() as u32, -, self, cpsr, S),
            0b0111 => fl!(op2, rn, !self.cpsr.c() as u32, -, self, cpsr, S),
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
                if !self
                    .cpsr
                    .mode()
                    .is_ok_and(|m| m == Mode::User || m == Mode::System)
                {
                    let spsr = self.spsr;
                    if self.cpsr.mode() != self.spsr.mode() {
                        self.swap_regs(self.cpsr.mode().unwrap(), self.spsr.mode().unwrap());
                    }
                    self.cpsr.set_cpsr(spsr.cpsr());
                }
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

        // Don't set result in rd when opcode is CMP, TST, TEQ, CMN.
        if !is_intmd {
            self.branch = rd == 15;
            self.regs[rd] = result;
        }
    }

    /// MUL and MLA. (check for r15 and rd != rm?)
    pub fn multiply<const S: bool>(&mut self, opcode: u32) {
        let acc = (opcode & (1 << 21)) != 0;

        let rd = (opcode as usize & 0x000F_0000) >> 16;
        let rm = self.regs[opcode as usize & 0xF];
        let rs = self.regs[(opcode as usize & 0x0F00) >> 8];
        let rn = self.regs[(opcode as usize & 0xF000) >> 12];

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

        let rd_hi_lo = ((self.regs[rd_hi] as u64) << 32) | self.regs[rd_lo] as u64;

        let res = match signed {
            false => rm as u64 * rs as u64 + (rd_hi_lo * acc as u64),
            true => ((rm as i32 as i64 * rs as i32 as i64) + (rd_hi_lo as i64 * acc as i64)) as u64,
        };

        self.regs[rd_hi] = (res >> 32) as u32;
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
                let (aligned_addr, data_ror) = if rn % 4 != 0 {
                    (rn & !3, (rn & 3) * 8)
                } else {
                    (rn, 0)
                };

                let swp_content = self.bus.read32(aligned_addr);
                self.bus.write32(aligned_addr, rm);
                self.regs[rd] = swp_content.rotate_right(data_ror);
            }
            true => {
                let swp_content = self.bus.read8(rn);
                self.bus.write8(rn, rm as u8);
                self.regs[rd] = swp_content as u32;
            }
        }
    }

    /// Branch and Exchange.
    pub fn bx(&mut self, opcode: u32) {
        let rn = self.regs[opcode as usize & 0xF];
        self.regs[15] = rn & !1;

        // Bit 0 of Rn decides decoding of subsequent instructions.
        match rn & 1 == 0 {
            true => self.cpsr.set_state(State::Arm),
            false => self.cpsr.set_state(State::Thumb),
        };

        self.branch = true;
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

        if link {
            self.regs[14] = self.regs[15] + 4;
        }

        self.branch = true;
        self.regs[15] = ((self.regs[15] + 8).wrapping_add_signed(ioffset)) & !3;
    }

    /// PSR Transfer. Transfer contents of CPSR/SPSR between registers.
    pub fn psr_transfer<const I: bool, const PSR: bool>(&mut self, opcode: u32) {
        let mut source_psr = match PSR {
            true => self.spsr,
            false => self.cpsr,
        };

        // MRS (transfer PSR contents to register)
        if (opcode & (1 << 21)) == 0 {
            let rd = (opcode as usize & 0xF000) >> 12;
            self.regs[rd] = source_psr.cpsr();
        }
        // MSR (transfer register contents to PSR, or #imm/#reg to flag bits)
        else {
            let rm = if !I {
                self.regs[opcode as usize & 0xF]
            } else {
                self.barrel_shifter::<I>(opcode as u16).0
            };

            // Get current mode before possible CPSR change.
            let Ok(current_mode) = self.cpsr.mode() else {
                return;
            };

            // User mode can only change flag bits.
            if self.cpsr.mode().is_ok_and(|mode| mode == Mode::User) {
                source_psr.set_cpsr((rm & 0xF000_0000) | (source_psr.cpsr() & 0x0FFF_FFFF));
            } else {
                // Set flag bits.
                if opcode & (1 << 19) != 0 {
                    source_psr.set_cpsr((rm & 0xF000_0000) | (source_psr.cpsr() & 0x0FFF_FFFF));
                }
                // Set control bits.
                if opcode & (1 << 16) != 0 {
                    source_psr.set_cpsr((rm & 0x7F) | (source_psr.cpsr() & !0x7F));
                }
            }

            // Assign to correct PSR.
            match PSR {
                true => self.spsr = source_psr,
                false => self.cpsr = source_psr,
            }

            // If PSR = CPSR and modes differ and control bits get set, change mode.
            if let Ok(new_mode) = Mode::try_from(rm & 0x1F) {
                if !PSR && current_mode as u32 != (rm & 0x1F) && opcode & (1 << 16) != 0 {
                    self.swap_regs(current_mode, new_mode);
                    self.cpsr.set_mode(new_mode);
                }
            }
        }
    }

    /// Software Interrupt.
    pub fn swi(&mut self, _opcode: u32) {
        self.swap_regs(self.cpsr.mode().unwrap(), Mode::Supervisor);
        self.cpsr.set_mode(Mode::Supervisor);

        self.spsr = self.cpsr;
        self.branch = true;

        self.regs[14] = self.regs[15] + 4;
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

        let pc = if rn == 15 { 8 } else { 0 };
        let base_with_offset = if U {
            self.regs[rn] + pc + offset
        } else {
            self.regs[rn] + pc - offset
        };

        let address = if P {
            base_with_offset
        } else {
            self.regs[rn] + pc
        };

        let (aligned_addr, ror) = if !B && address % 4 != 0 {
            (address & !3, (address & 3) * 8)
        } else {
            (address, 0)
        };

        // Load from memory if L, else store register into memory.
        if L {
            self.branch = rd == 15;
            self.regs[rd] = if B {
                self.bus.read8(address) as u32
            } else {
                self.bus.read32(aligned_addr).rotate_right(ror)
            };
        } else {
            let data = if rd == 15 {
                self.regs[rd] + 12
            } else {
                self.regs[rd]
            };
            if B {
                self.bus.write8(address, data as u8);
            } else {
                self.bus.write32(aligned_addr, data);
            }
        }

        // TODO: simplify lmao
        if ((W || !P) && (rn != rd) && L) || (!L && (W || !P)) {
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
        let (aligned_addr, ror) = if address % 2 != 0 {
            (address & !1, 8)
        } else {
            (address, 0)
        };

        // Load from memory if L, else store register into memory.
        if L {
            if !S {
                self.regs[rd] = (self.bus.read16(aligned_addr) as u32).rotate_right(ror);
            } else {
                self.regs[rd] = match H {
                    false => self.bus.read8(address) as i8 as u32,
                    true if address % 2 != 0 => self.bus.read8(address) as i8 as u32,
                    true => self.bus.read16(address) as i16 as u32,
                }
            }
        } else {
            self.bus.write16(aligned_addr, self.regs[rd] as u16);
        }

        if ((W || !P) && (rn != rd)) || (!L && (W || !P)) {
            self.regs[rn] = base_with_offset;
        }
    }

    /// LDM/STM. (TODO: sys and user mode should be same)
    #[rustfmt::skip]
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

        // Edge case: PSR bit and r15 not in list.
        let user_bank = S && !reg_list.contains(&15);

        let mut address = self.regs[rn];
        // Force align address but not directly modify it -- writeback is not aligned.
        let aligned_addr = |address: u32| {
            if address % 4 != 0 {
                address & !3
            } else {
                address
            }
        };

        // Edge case: empty register list.
        if reg_list.is_empty() {
            address = match (U, P) {
                (false, false) => self.regs[rn] - 0x3C,
                (false, true) => self.regs[rn] - 0x40,
                (true, true) => self.regs[rn] + 0x4,
                (true, false) => self.regs[rn],
            };

            if L {
                self.branch = true;
                self.regs[15] = self.bus.read32(aligned_addr(address));
            } else {
                self.bus
                    .write32(aligned_addr(address), (self.regs[15] + 12) & !3);
            }

            self.regs[rn] = if U {
                self.regs[rn] + 0x40
            } else {
                self.regs[rn] - 0x40
            };
            return;
        }

        if !U {
            reg_list.reverse()
        }

        for r in &reg_list {
            if P {
                // Pre-{inc, dec}rement addressing.
                address = if U { address + 4 } else { address - 4 };
            }

            if L {
                // Edge case: PSR bit and r15 in list.
                if S && *r == 15 {
                    self.cpsr.set_cpsr(self.spsr.cpsr());
                }

                match user_bank {
                    false => self.regs[*r] = self.bus.read32(aligned_addr(address)),
                    true => {
                        self.banked_regs
                            .entry(Mode::System)
                            .and_modify(|(_, regs)| {
                                regs[*r] = self.bus.read32(aligned_addr(address))
                            });
                    }
                }
            } else {
                // Edge case: rb in reg list and not first.
                if *r == rn
                    && ((U && reg_list[0] != *r) || (!U && reg_list[reg_list.len() - 1] != *r))
                {
                    self.bus.write32(
                        aligned_addr(address),
                        if U {
                            self.regs[rn] + (reg_list.len() as u32 * 4)
                        } else {
                            self.regs[rn] - (reg_list.len() as u32 * 4)
                        },
                    )
                } else {
                    self.bus.write32(
                        aligned_addr(address),
                        if !user_bank {
                            self.regs[*r] + if *r == 15 { 12 } else { 0 }
                        } else {
                            self.banked_regs[&Mode::System].1[*r]
                        },
                    );
                }
            }

            if !P {
                // Post-{inc, dec}rement addressing.
                address = if U { address + 4 } else { address - 4 };
            }
        }

        self.branch = L && reg_list.contains(&15);
        // Writeback if W  and if Load but rn not in list or if Store and W.
        if (W && (L && !reg_list.contains(&rn))) || (!L && W) {
            self.regs[rn] = address;
        }
    }

    /// Test for LUT.
    pub fn undefined(&mut self, _opcode: u32) {
        panic!("shouldn't be called!")
    }

    // BARREL SHIFTER UTILITY METHODS.

    /// Logical shift left, returns result and carry out.
    #[inline(always)]
    pub(super) fn lsl(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
        match reg {
            false => (rm << amount, rm & (1 << (32 - amount)) != 0),
            true => {
                if amount == 0 {
                    (rm, self.cpsr.c())
                } else if amount < 32 {
                    (rm << amount, rm & (1 << (32 - amount)) != 0)
                } else if amount == 32 {
                    (0, (rm & 1) != 0)
                } else {
                    (0, false)
                }
            }
        }
    }

    /// Logical shift right, returns result and carry out.
    #[inline(always)]
    pub(super) fn lsr(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
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
                } else if amount == 32 {
                    (0, (rm >> 31) != 0)
                } else {
                    (0, false)
                }
            }
        }
    }

    /// Arithmetic shift right, returns result and carry out.
    #[inline(always)]
    pub(super) fn asr(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
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
    pub(super) fn ror(&self, rm: u32, amount: u32, reg: bool) -> (u32, bool) {
        if amount == 0 {
            if reg {
                return (rm, self.cpsr.c());
            } else {
                return ((self.cpsr.c() as u32) << 31 | (rm >> 1), (rm & 1) != 0);
            }
        }

        (rm.rotate_right(amount), rm & (1 << (amount - 1)) != 0)
    }

    /// Swap banked registers on mode change. Call before changing mode in CPSR.
    fn swap_regs(&mut self, current_mode: Mode, new_mode: Mode) {
        let (spsr_mode, bank_regs) = self
            .banked_regs
            .get(&new_mode)
            .cloned()
            .unwrap_or((Cpsr(0), [0; 16]));

        self.banked_regs
            .insert(current_mode, (self.spsr, self.regs));
        self.spsr = spsr_mode;

        if current_mode == Mode::Fiq {
            self.regs[8..=14].copy_from_slice(&bank_regs[8..=14]);
        } else {
            self.regs[13..=14].copy_from_slice(&bank_regs[13..=14]);
        }
    }
}
