use crate::mmu::Mcu;

use super::arm7tdmi::Arm7TDMI;

// TODO: flags for CPSR!
/// Thumb instructions live in this impl block.
impl Arm7TDMI {
    /// Format 1: move shifted register.
    pub fn mov_shifted_reg(&mut self, opcode: u16) {
        let rd = opcode & 0x7;
        let rs = (opcode >> 3) & 0x7;

        let offset = (opcode >> 6) & 0x1F;
        let op = (opcode >> 11) & 0x3;

        self.regs[rd as usize] = match op {
            0b00 => self.regs[rs as usize] << offset,
            0b01 => self.regs[rs as usize] >> offset,
            0b10 => {
                let bit31 = self.regs[rs as usize] & (1 << 31);
                let mut res = self.regs[rs as usize] >> offset;

                for i in 0..offset {
                    res |= bit31 >> i;
                }

                res
            }
            _ => unreachable!(),
        };
    }

    /// Format 2: add/substract.
    pub fn add_sub<const I: bool>(&mut self, opcode: u16) {
        let rd = opcode & 0x7;
        let rs = (opcode >> 3) & 0x7;

        let offset = if I {
            (opcode as u32 >> 6) & 0x7
        } else {
            self.regs[(opcode as usize >> 6) & 0x7]
        };

        self.regs[rd as usize] = match (opcode >> 9) & 1 {
            0 => self.regs[rs as usize] + offset,
            1 => self.regs[rs as usize] - offset,
            _ => unreachable!(),
        };
    }

    /// Format 3: move/compare/add/substract immediate.
    pub fn mov_cmp_alu_imm(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let rd = (opcode as usize >> 8) & 0x7;

        self.regs[rd] = match (opcode >> 11) & 0x3 {
            0b00 => offset,
            0b01 => todo!("cmp"),
            0b10 => self.regs[rd] + offset,
            0b11 => self.regs[rd] - offset,
            _ => unreachable!(),
        };
    }

    /// Format 4: ALU operations.
    pub fn alu_ops(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rs = (opcode as usize >> 3) & 0x7;

        // If intermediate: TST, CMP, CMN.
        let mut intmd = false;

        let res = match (opcode >> 6) & 0xF {
            0b0000 => self.regs[rd] & self.regs[rs],
            0b0001 => self.regs[rd] ^ self.regs[rs],
            0b0010 => self.regs[rd] << self.regs[rs],
            0b0011 => self.regs[rd] >> self.regs[rs],
            0b0100 => todo!("asr"),
            0b0101 => self.regs[rd] + self.regs[rs] + self.cpsr.c() as u32,
            0b0110 => self.regs[rd] - self.regs[rs] - (!self.cpsr.c()) as u32,
            0b0111 => self.regs[rd].rotate_right(self.regs[rs]),
            0b1000 => {
                intmd = true;
                todo!("tst")
            }
            0b1001 => self.regs[rs].wrapping_neg(),
            0b1010 => {
                intmd = true;
                todo!("cmp")
            }
            0b1011 => {
                intmd = true;
                todo!("cmn")
            }
            0b1100 => self.regs[rd] | self.regs[rs],
            0b1101 => self.regs[rd] * self.regs[rs],
            0b1110 => self.regs[rd] & !self.regs[rs],
            0b1111 => !self.regs[rs],
            _ => unreachable!(),
        };

        if !intmd {
            self.regs[rd] = res;
        }
    }

    // TODO: Format 5.

    /// Format 6: PC-relative load.
    pub fn pc_rel_load(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let rd = (opcode as usize >> 8) & 0x7;

        self.regs[rd] = self.bus.read32(self.regs[15] + offset + 4);
    }

    /// Format 7: load/store with register offset.
    pub fn load_store_reg<const L: bool, const B: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rb = (opcode as usize >> 3) & 0x7;
        let ro = (opcode as usize >> 6) & 0x7;

        let address = self.regs[rb] + self.regs[ro];

        if L {
            self.regs[rd] = if B {
                self.bus.read8(address) as u32
            } else {
                self.bus.read32(address)
            };
        } else {
            match B {
                false => self.bus.write32(address, self.regs[rd]),
                true => self.bus.write8(address, self.regs[rd] as u8),
            }
        }
    }

    /// Format 8: load/store sign-extended byte/halfword.
    pub fn load_store_hw_signext<const H: bool, const S: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rb = (opcode as usize >> 3) & 0x7;
        let ro = (opcode as usize >> 6) & 0x7;

        let address = self.regs[rb] + self.regs[ro];

        match (S, H) {
            (false, false) => self.bus.write16(address, self.regs[rd] as u16),
            (false, true) => self.regs[rd] = self.bus.read16(address) as u32,
            (true, false) => self.regs[rd] = self.bus.read8(address) as i32 as u32,
            (true, true) => self.regs[rd] = self.bus.read16(address) as i32 as u32,
        };
    }

    /// Format 9: load/store with immediate offset.
    pub fn load_store_imm<const L: bool, const B: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rb = (opcode as usize >> 3) & 0x7;
        let offset = (opcode as u32 >> 6) & 0x1F;

        let address = self.regs[rb] + offset;

        if L {
            self.regs[rd] = if B {
                self.bus.read8(address) as u32
            } else {
                self.bus.read32(address)
            };
        } else {
            match B {
                false => self.bus.write32(address, self.regs[rd]),
                true => self.bus.write8(address, self.regs[rd] as u8),
            }
        }
    }

    /// Format 10: load/store halfword.
    pub fn load_store_hw<const L: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rb = (opcode as usize >> 3) & 0x7;
        let offset = (opcode as u32 >> 6) & 0x1F;

        let address = self.regs[rb] + offset;

        if L {
            self.regs[rd] = self.bus.read16(address) as u32;
        } else {
            self.bus.write16(address, self.regs[rd] as u16);
        }
    }

    /// Format 11: SP-relative load/store.
    pub fn sp_rel_load_store<const L: bool>(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let rd = (opcode as usize >> 8) & 0x7;

        if L {
            self.regs[rd] = self.bus.read32(self.regs[13] + offset);
        } else {
            self.bus.write32(self.regs[13] + offset, self.regs[rd]);
        }
    }

    /// Format 12: load address.
    pub fn load_addr<const SP: bool>(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let rd = (opcode as usize >> 8) & 0x7;

        self.regs[rd] = match SP {
            false => ((self.regs[15] + 4) & !1) + (offset << 2),
            true => self.regs[13] + (offset << 2),
        };
    }

    /// Format 13: add offset to SP. (todo: 9 bit constant?)
    pub fn add_sp<const S: bool>(&mut self, opcode: u16) {
        let offset = (opcode & 0x7F) as i8 as i32;

        if S {
            self.regs[13] -= offset as u32;
        } else {
            self.regs[13] = self.regs[13].wrapping_add_signed(offset);
        }
    }

    // Format 14.
    // Format 15.

    /// Format 16: conditional branch.
    pub fn cond_branch(&mut self, opcode: u16) {
        let signed_offset = opcode as u8 as i32;

        if self.cond((opcode >> 8) as u8 & 0xF) {
            self.regs[15] = self.regs[15].wrapping_add_signed(signed_offset + 4);
        }
    }

    // Format 17: swi (same as ARM)

    /// Format 18: unconditional branch. (shift left?)
    pub fn branch(&mut self, opcode: u16) {
        let signed_offset = (opcode & 0x7F) as i32;
        self.regs[15] = self.regs[15].wrapping_add_signed(signed_offset + 4);
    }

    /// Format 19: long branch with link.
    pub fn long_branch<const H: bool>(&mut self, opcode: u16) {
        let offset = opcode & 0x7F;

        if !H {
            self.regs[14] = self.regs[15] + ((offset as u32) << 12);
        } else {
            let addr = self.regs[14] + ((offset as u32) << 1);
            self.regs[14] = (self.regs[15] + 4) | 1;
            self.regs[15] = addr + 4;
        }
    }
}
