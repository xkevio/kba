//! For the jitted ARM32 instructions.

use cranelift::codegen::ir::{types::{I16, I32, I64, I8}, Block, InstBuilder};
use cranelift_module::Module;

use crate::{arm::interpreter::arm7tdmi::{Arm7TDMI, State}, mmu::Mcu};

use super::JitTranslator;

impl Arm7TDMI {
    pub fn get_next_block(&mut self, jit_translator: &mut JitTranslator) -> (Block, usize) {
        if jit_translator.blocks.contains_key(&self.regs[15]) {
            return jit_translator.blocks[&self.regs[15]];
        }

        let mut cycles = 0;
        let block = jit_translator.builder.create_block();
        jit_translator.builder.switch_to_block(block);

        while !self.branch {
            match self.cpsr.state() {
                State::Arm => {
                    let opcode = self.bus.read32(self.regs[15]);
    
                    let cond = (opcode >> 28) & 0xF;
                    let op_index = ((opcode & 0x0FF0_0000) >> 16) | ((opcode & 0x00F0) >> 4);
    
                    if self.cond(cond as u8) {
                        // TODO: ARM_JIT_LUT that modifies `Block`!
                        // ARM_INSTRUCTIONS[op_index as usize](self, opcode);
                    }
                }
                State::Thumb => {
                    let opcode = self.bus.read16(self.regs[15]);
                    // TODO: THUMB_JIT_LUT that modifies `Block`!
                    // THUMB_INSTRUCTIONS[(opcode >> 8) as usize](self, opcode);
                }
            }

            cycles += 1;
            // TODO: Increase PC.
        }

        // todo: seal block?
        jit_translator.blocks.insert(self.regs[15], (block, cycles));
        (block, cycles)
    }

    pub fn data_processing_jit<const I: bool, const S: bool>(&mut self, opcode: u32, jit: &mut JitTranslator) {
        let rd = (opcode as usize & 0xF000) >> 12;
        let rn = self.regs[(opcode as usize & 0x000F_0000) >> 16];
        let (op2, carry_out) = self.barrel_shifter::<I>(opcode as u16);

        let clir = &mut jit.builder;

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

        let rn_v = clir.ins().iconst(I64, rn as i64);
        let op2_v = clir.ins().iconst(I64, op2 as i64);

        let result = match operation {
            0b0000 => clir.ins().band(rn_v, op2_v),
            0b0001 => clir.ins().bxor(rn_v, op2_v),
            0b0010 => clir.ins().usub_overflow(rn_v, op2_v).0,
            0b0011 => clir.ins().usub_overflow(op2_v, rn_v).0,
            0b0100 => clir.ins().uadd_overflow(rn_v, op2_v).0,
            0b0101 => todo!("ADC"),
            0b0110 => todo!("SBC"),
            0b0111 => todo!("RSC"),
            0b1000 => {
                is_intmd = true;
                clir.ins().band(rn_v, op2_v)
            },
            0b1001 => {
                is_intmd = true;
                clir.ins().bxor(rn_v, op2_v)
            },
            0b1010 => {
                is_intmd = true;
                clir.ins().usub_overflow(rn_v, op2_v).0
            },
            0b1011 => {
                is_intmd = true;
                clir.ins().uadd_overflow(rn_v, op2_v).0
            },
            0b1100 => clir.ins().bor(rn_v, op2_v),
            0b1101 => op2_v,
            0b1110 => clir.ins().band_not(rn_v, op2_v),
            0b1111 => clir.ins().bnot(op2_v),
            _ => unreachable!()
        };

        if S {
            // Do r15 shenanigans and set flags in CPSR.
        }

        if !is_intmd {
            self.branch = rd == 15;
            clir.def_var(jit.regs[rd], result);
        }
    }

    pub fn bl_jit(&mut self, opcode: u32, jit: &mut JitTranslator) {
        let offset = (opcode & 0x00FF_FFFF) << 2;
        let link = opcode & (1 << 24) != 0;
        let signed_off = (offset << 6) as i32 >> 6;

        let clir = &mut jit.builder;
        let eight = clir.ins().iconst(I64, 8);
        let r15 = clir.use_var(jit.regs[15]);

        if link {
            let four = clir.ins().iconst(I64, 4);
            let (r15_new, _) = clir.ins().uadd_overflow(r15, four);
            let r14_new = clir.ins().band_imm(r15_new, !3);
            
            clir.def_var(jit.regs[14], r14_new);
        }

        self.branch = true;

        let r15_new = {
            let tmp = clir.ins().uadd_overflow(r15, eight);
            let pc_unaligned = clir.ins().iadd_imm(tmp.0, signed_off as i64);
            clir.ins().band_imm(pc_unaligned, !3)
        };
        clir.def_var(jit.regs[15], r15_new);
    }

    pub fn hw_signed_data_transfer_jit<
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
        jit: &mut JitTranslator,
    ) {
        let clir = &mut jit.builder;
        let rn = (opcode as usize & 0x000F_0000) >> 16;
        let rd = (opcode as usize & 0xF000) >> 12;

        let offset = if I {
            clir.ins().iconst(I32, (((opcode & 0xF00) >> 4) | (opcode & 0xF)) as i64)
        } else {
            clir.use_var(jit.regs[opcode as usize & 0xF])
        };

        let mut base_with_offset = if U {
            let reg_n = clir.use_var(jit.regs[rn]);
            clir.ins().uadd_overflow(reg_n, offset).0
        } else {
            let reg_n = clir.use_var(jit.regs[rn]);
            clir.ins().usub_overflow(reg_n, offset).0
        };

        base_with_offset = clir.ins().iadd_imm(base_with_offset, (rn == 15) as i64 * 8);

        let address = if P { base_with_offset } else { clir.use_var(jit.regs[rn]) };
        let address_cond = clir.ins().urem_imm(address, 2);

        // Align address if necessary and return alongside ror value. No clue if this works.
        let (aligned_addr, ror) = {
            let zero = clir.ins().iconst(I32, 0);
            let eight = clir.ins().iconst(I32, 8);
            let tmp = clir.ins().band_imm(address, !1);

            (
                clir.ins().select(address_cond, tmp, address),
                clir.ins().select(address_cond, eight, zero)
            )
        };

        let r8_func = jit.module.declare_func_in_func(jit.io[0], clir.func);
        let r16_func = jit.module.declare_func_in_func(jit.io[1], clir.func);
        let w16_func = jit.module.declare_func_in_func(jit.io[4], clir.func);

        if L {
            if !S {
                let mem16 = clir.ins().call(r8_func, &[aligned_addr]);
                let mem16 = {
                    let tmp = clir.inst_results(mem16)[0];
                    clir.ins().rotr(tmp, ror)
                };

                clir.def_var(jit.regs[15], mem16);
            } else {
                let res = match H {
                    false => {
                        let mem8 = clir.ins().call(r8_func, &[address]);
                        let mem8 = clir.inst_results(mem8)[0];
                        clir.ins().sextend(I8, mem8)
                    }
                    true => {
                        let mem8 = {
                            let mem8 = clir.ins().call(r8_func, &[address]);
                            let mem8 = clir.inst_results(mem8)[0];
                            clir.ins().sextend(I8, mem8)
                        };

                        let mem16 = {
                            let mem16 = clir.ins().call(r16_func, &[address]);
                            let mem16 = clir.inst_results(mem16)[0];
                            clir.ins().sextend(I16, mem16)
                        };

                        clir.ins().select(address_cond, mem8, mem16)
                    }
                };

                clir.def_var(jit.regs[rd], res);
            }
        } else {
            let rdv = clir.use_var(jit.regs[rd]);
            let value = clir.ins().iadd_imm(rdv, if rd == 15 { 12 } else { 0 });
            let _ = clir.ins().call(w16_func, &[aligned_addr, value]);
        }

        self.branch = rd == 15 && L;
        if ((W || !P) && (rn != rd)) || (!L && (W || !P)) {
            clir.def_var(jit.regs[rn], base_with_offset);
        }
    }
}