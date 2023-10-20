use crate::{fl, mmu::Mcu};

use super::arm7tdmi::{Arm7TDMI, State};

/// Thumb instructions live in this impl block.
impl Arm7TDMI {
    /// Format 1: move shifted register.
    pub fn mov_shifted_reg(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rs = (opcode as usize >> 3) & 0x7;

        let offset = (opcode >> 6) & 0x1F;
        let op = (opcode >> 11) & 0x3;

        self.regs[rd] = match op {
            0b00 => {
                let (res, carry) = self.lsl(self.regs[rs], offset as u32, false);
                self.cpsr.set_c(carry);
                res
            }
            0b01 => {
                let (res, carry) = self.lsr(self.regs[rs], offset as u32, false);
                self.cpsr.set_c(carry);
                res
            }
            0b10 => {
                let (res, carry) = self.asr(self.regs[rs], offset as u32, false);
                self.cpsr.set_c(carry);
                res
            }
            _ => unreachable!(),
        };

        self.cpsr.set_z(self.regs[rd] == 0);
        self.cpsr.set_n((self.regs[rd] & (1 << 31)) != 0);
    }

    /// Format 2: add/substract.
    pub fn add_sub<const I: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rs = (opcode as usize >> 3) & 0x7;

        let offset = if I {
            (opcode as u32 >> 6) & 0x7
        } else {
            self.regs[(opcode as usize >> 6) & 0x7]
        };

        self.regs[rd] = match (opcode >> 9) & 1 {
            0 => fl!(self.regs[rs], offset, +, self, cpsr),
            1 => fl!(self.regs[rs], offset, -, self, cpsr),
            _ => unreachable!(),
        };

        self.cpsr.set_z(self.regs[rd] == 0);
        self.cpsr.set_n((self.regs[rd] & (1 << 31)) != 0);
    }

    /// Format 3: move/compare/add/substract immediate.
    pub fn mov_cmp_alu_imm(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let rd = (opcode as usize >> 8) & 0x7;

        self.regs[rd] = match (opcode >> 11) & 0x3 {
            0b00 => offset,
            0b01 => {
                let cmp_res = fl!(self.regs[rd], offset, -, self, cpsr);

                self.cpsr.set_z(cmp_res == 0);
                self.cpsr.set_n((cmp_res & (1 << 31)) != 0);

                self.regs[rd]
            }
            0b10 => fl!(self.regs[rd], offset, +, self, cpsr),
            0b11 => fl!(self.regs[rd], offset, -, self, cpsr),
            _ => unreachable!(),
        };

        // If NOT cmp.
        if (opcode >> 11) & 0x3 != 0b01 {
            self.cpsr.set_z(self.regs[rd] == 0);
            self.cpsr.set_n((self.regs[rd] & (1 << 31)) != 0);
        }
    }

    /// Format 4: ALU operations.
    pub fn alu_ops(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rs = (opcode as usize >> 3) & 0x7;

        // If intermediate: TST, CMP, CMN.
        let mut intmd = false;

        #[rustfmt::skip]
        let res = match (opcode >> 6) & 0xF {
            0b0000 => self.regs[rd] & self.regs[rs],
            0b0001 => self.regs[rd] ^ self.regs[rs],
            0b0010 => {
                let (res, carry) = self.lsl(self.regs[rd], self.regs[rs], true);
                self.cpsr.set_c(carry);
                res
            }
            0b0011 => {
                let (res, carry) = self.lsr(self.regs[rd], self.regs[rs], true);
                self.cpsr.set_c(carry);
                res
            }
            0b0100 => {
                let (res, carry) = self.asr(self.regs[rd], self.regs[rs], true);
                self.cpsr.set_c(carry);
                res
            }
            0b0101 => fl!(self.regs[rd], self.regs[rs] + self.cpsr.c() as u32, +, self, cpsr),
            0b0110 => fl!(self.regs[rd], self.regs[rs], (!self.cpsr.c()) as u32, -, self, cpsr),
            0b0111 => {
                let (res, carry) = self.ror(self.regs[rd], self.regs[rs], true);
                self.cpsr.set_c(carry);
                res
            },
            0b1000 => { intmd = true; self.regs[rd] & self.regs[rs] },
            0b1001 => fl!(0, self.regs[rs], -, self, cpsr),
            0b1010 => { intmd = true; fl!(self.regs[rd], self.regs[rs], -, self, cpsr) },
            0b1011 => { intmd = true; fl!(self.regs[rd], self.regs[rs], +, self, cpsr) },
            0b1100 => self.regs[rd] | self.regs[rs],
            0b1101 => self.regs[rd] * self.regs[rs],
            0b1110 => self.regs[rd] & !self.regs[rs],
            0b1111 => !self.regs[rs],
            _ => unreachable!(),
        };

        self.cpsr.set_z(res == 0);
        self.cpsr.set_n((res & (1 << 31)) != 0);

        if !intmd {
            self.regs[rd] = res;
        }
    }

    /// Format 5: Hi reg ops/bx
    #[rustfmt::skip]
    pub fn hi_reg_op_bx(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rs = (opcode as usize >> 3) & 0x7;
        let op = (opcode >> 8) & 0x3;

        // No const generic if decoding with 8 bits :(
        let h1 = opcode & (1 << 7) != 0;
        let h2 = opcode & (1 << 6) != 0;

        // Branch exchange.
        if op == 0b11 {
            let addr = if !h2 { self.regs[rs] } else { self.regs[rs + 8] };
            self.regs[15] = addr & !1;

            // Bit 0 of Rn decides decoding of subsequent instructions.
            if addr & 1 == 0 {
                self.cpsr.set_state(State::Arm);
            } else {
                self.cpsr.set_state(State::Thumb);
                self.branch = true;
            }

            return;
        }

        let dst = if !h1 { rd } else { rd + 8 };
        let src = if !h2 { rs } else { rs + 8 };
        let pc = if rs == 15 { 4 } else { 0 };

        self.regs[dst] = match op {
            0b00 => self.regs[dst] + self.regs[src] + pc,
            0b01 => { fl!(self.regs[dst], self.regs[src] + pc, -, self, cpsr); self.regs[dst] },
            0b10 => self.regs[src] + pc,
            _ => unreachable!(),
        };
    }

    /// Format 6: PC-relative load.
    pub fn pc_rel_load(&mut self, opcode: u16) {
        let offset = (opcode as u8 as u32) << 2;
        let rd = (opcode as usize >> 8) & 0x7;

        self.regs[rd] = self.bus.read32(((self.regs[15] + 4) & !2) + offset);
    }

    /// Format 7: load/store with register offset.
    pub fn load_store_reg<const L: bool, const B: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rb = (opcode as usize >> 3) & 0x7;
        let ro = (opcode as usize >> 6) & 0x7;

        let address = self.regs[rb] + self.regs[ro];
        let (aligned_addr, ror) = if !B && address % 4 != 0 {
            (address & !3, (address & 3) * 8)
        } else {
            (address, 0)
        };

        if L {
            self.regs[rd] = if B {
                self.bus.read8(address) as u32
            } else {
                self.bus.read32(aligned_addr).rotate_right(ror)
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
            (true, false) => self.regs[rd] = self.bus.read8(address) as i8 as u32,
            (true, true) => self.regs[rd] = self.bus.read16(address) as i16 as u32,
        };
    }

    /// Format 9: load/store with immediate offset.
    pub fn load_store_imm<const L: bool, const B: bool>(&mut self, opcode: u16) {
        let rd = opcode as usize & 0x7;
        let rb = (opcode as usize >> 3) & 0x7;
        let offset = (opcode as u32 >> 6) & 0x1F;

        let address = self.regs[rb] + (offset << if B { 0 } else { 2 });
        let (aligned_addr, ror) = if !B && address % 4 != 0 {
            (address & !3, (address & 3) * 8)
        } else {
            (address, 0)
        };

        if L {
            self.regs[rd] = if B {
                self.bus.read8(address) as u32
            } else {
                self.bus.read32(aligned_addr).rotate_right(ror)
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

        let address = self.regs[rb] + (offset << 1);

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
            self.regs[rd] = self.bus.read32(self.regs[13] + (offset << 2));
        } else {
            self.bus
                .write32(self.regs[13] + (offset << 2), self.regs[rd]);
        }
    }

    /// Format 12: load address.
    pub fn load_addr<const SP: bool>(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let rd = (opcode as usize >> 8) & 0x7;

        self.regs[rd] = match SP {
            false => ((self.regs[15] + 4) & !2) + (offset << 2),
            true => self.regs[13] + (offset << 2),
        };
    }

    /// Format 13: add offset to SP. (todo: 9 bit constant?)
    pub fn add_sp(&mut self, opcode: u16) {
        let offset = (opcode & 0x7F) as i8 as i32;
        let s = opcode & (1 << 7) != 0;

        // No const generic with current 8bit thumb decoding :(
        if s {
            self.regs[13] -= (offset << 2) as u32;
        } else {
            self.regs[13] = self.regs[13].wrapping_add_signed(offset << 2);
        }
    }

    /// Format 14: push/pop registers.
    pub fn push_pop<const L: bool, const R: bool>(&mut self, opcode: u16) {
        let mut reg_list = (0..=7)
            .filter(|i| (opcode & (1 << i)) != 0)
            .collect::<Vec<_>>();

        let mut address = self.regs[13];
        if !L {
            reg_list.reverse()
        }

        if R && !L {
            address -= 4;
            self.bus.write32(address, self.regs[14])
        }

        for r in &reg_list {
            if L {
                self.regs[*r] = self.bus.read32(address);
                address += 4;
            } else {
                address -= 4;
                self.bus.write32(address, self.regs[*r]);
            }
        }

        if R && L {
            self.regs[15] = self.bus.read32(address) & !1;
            self.branch = true;
            address += 4;
        }

        // Writeback if r13 not in reg list.
        if (L && !reg_list.contains(&13)) || !L {
            self.regs[13] = address;
        }
    }

    /// Format 15: multiple load/store
    pub fn ldm_stm<const L: bool>(&mut self, opcode: u16) {
        let mut reg_list = (0..=7)
            .filter(|i| (opcode & (1 << i)) != 0)
            .collect::<Vec<_>>();

        let rb = (opcode as usize >> 8) & 0x7;
        let mut address = self.regs[rb];
        if !L {
            reg_list.reverse()
        }

        for r in &reg_list {
            if L {
                self.regs[*r] = self.bus.read32(address);
            } else {
                self.bus.write32(address, self.regs[*r]);
            }

            address += 4
        }

        // Writeback if r13 not in reg list.
        if (L && !reg_list.contains(&rb)) || !L {
            self.regs[rb] = address;
        }
    }

    /// Format 16: conditional branch.
    pub fn cond_branch(&mut self, opcode: u16) {
        let signed_offset = ((opcode & 0xFF) << 1) as i8 as i32;

        if self.cond((opcode >> 8) as u8 & 0xF) {
            self.regs[15] = self.regs[15].wrapping_add_signed(signed_offset + 4 - 2);
        }
    }

    /// Format 17: swi (same as ARM)
    pub fn t_swi(&mut self, _opcode: u16) {
        self.swi(0);
    }

    /// Format 18: unconditional branch.
    pub fn branch(&mut self, opcode: u16) {
        let signed_offset = (((opcode as u32 & 0x7FF) << 1) << 22) as i32 >> 22;
        self.regs[15] = self.regs[15].wrapping_add_signed(signed_offset + 4 - 2);
    }

    /// Format 19: long branch with link.
    pub fn long_branch<const H: bool>(&mut self, opcode: u16) {
        let offset = opcode & 0x7FF;

        if !H {
            // Sign extend top half, shift by 12 offset bcs of prev shift.
            let s_off = (((offset as u32) << 22) as i32 >> 22) << 12;
            self.regs[14] = (self.regs[15] + 4).wrapping_add_signed(s_off);
        } else {
            let addr = self.regs[14] + ((offset << 1) as u32);

            self.regs[14] = (self.regs[15] + 2) | 1;
            self.regs[15] = addr;

            self.branch = true;
        }
    }

    /// Dummy for Thumb LUT.
    pub fn t_dummy(&mut self, _opcode: u16) {}
}
