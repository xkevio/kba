//! For the jitted ARM32 instructions.

use cranelift::{codegen::ir::{types::{I16, I32, I64, I8}, AbiParam, InstBuilder, MemFlags, Value}, frontend::{FunctionBuilder, FunctionBuilderContext}};
use cranelift_module::{Module, Linkage};

use crate::{arm::interpreter::arm7tdmi::{Arm7TDMI, State}, mmu::Mcu};

use super::JitContext;

include!(concat!(env!("OUT_DIR"), "/arm_instructions_jit.rs"));

impl Arm7TDMI {
    pub fn get_next_block(&mut self, jit: &mut JitContext) -> (*const u8, usize) {
        let beginning_of_block = self.regs[15];
        if jit.blocks.contains_key(&self.regs[15]) {
            return jit.blocks[&self.regs[15]].clone();
        }

        let mut ctx = jit.module.make_context();
        ctx.func.signature.params.push(AbiParam::new(jit.module.isa().pointer_type()));
        ctx.func.signature.returns.push(AbiParam::new(jit.module.isa().pointer_type()));
        
        ctx.set_disasm(true);

        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        
        let mut cycles = 0;
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);

        while !self.branch {
            match self.cpsr.state() {
                State::Arm => {
                    let opcode = self.bus.read32(self.regs[15]);

                    let _cond = (opcode >> 28) & 0xF;
                    let op_index = ((opcode & 0x0FF0_0000) >> 16) | ((opcode & 0x00F0) >> 4);

                    ARM_INSTRUCTIONS_JIT[op_index as usize](self, opcode, jit, &mut builder);
                }
                State::Thumb => {
                    // let opcode = self.bus.read16(self.regs[15]);
                    // TODO: THUMB_JIT_LUT that modifies `Block`!
                    // THUMB_INSTRUCTIONS[(opcode >> 8) as usize](self, opcode);
                }
            }

            cycles += 1;

            match self.cpsr.state() {
                State::Arm if !self.branch => {
                    let pc_old = self.reg(&mut builder, 15);
                    let pc_new = builder.ins().iadd_imm(pc_old, 4);

                    self.set_reg(&mut builder, 15, pc_new);
                    self.regs[15] += 4;
                },
                State::Thumb if !self.branch => {
                    let pc_old = self.reg(&mut builder, 15);
                    let pc_new = builder.ins().iadd_imm(pc_old, 2);
                    
                    self.set_reg(&mut builder, 15, pc_new);
                    self.regs[15] += 2;
                },
                _ => {},
            }
        }

        let result = builder.block_params(block)[0];
        
        builder.ins().return_(&[result]);
        builder.seal_block(block);
        builder.finalize();
        
        self.branch = false;
        // println!("{}", ctx.func.display());
        
        let id = jit.module.declare_function(&format!("{:X}", beginning_of_block), Linkage::Export, &ctx.func.signature).unwrap();
        jit.module.define_function(id, &mut ctx).unwrap();
        // println!("define and compile");
        jit.module.clear_context(&mut ctx);
        // println!("clear context");
        jit.module.finalize_definitions().unwrap();
        // println!("relocation");

        let code = jit.module.get_finalized_function(id);
        jit.blocks.insert(beginning_of_block, (code, cycles));
        (code, cycles)
    }

    pub fn data_processing_jit<const I: bool, const S: bool>(&mut self, opcode: u32, _jit: &mut JitContext, builder: &mut FunctionBuilder) {
        let rd = (opcode as usize & 0xF000) >> 12;
        let rn_v = self.reg(builder, (opcode as i32 & 0x000F_0000) >> 16);
        let is_shift = ((opcode & 0xFFFF_0000) >> 16) == 0xE1A0; 

        let op2_v = if !I {
            let op = opcode as u16;
            let amount = if is_shift { (op as u32 & 0x0F80) >> 7 } else { 0 };
            let rm_val = self.reg(builder, op as i32 & 0xF);
            builder.ins().ishl_imm(rm_val, amount as i64)
        } else {
            let ror = (opcode >> 8) & 0xF;
            let x1 = builder.ins().iconst(I32, opcode as i64 & 0xFF);
            builder.ins().rotr_imm(x1, ror as i64 * 2)
        };

        let clir = builder;

        // Bits 21-24 specify the actual opcode.
        let operation = (opcode & 0x01E0_0000) >> 21;
        // Check if TST, TEQ, CMP, CMN.
        let mut is_intmd = false;
        // If operand is PC, add 8.
        // let rn = if (opcode & 0x000F_0000) >> 16 == 15 {
        //     if !I && (opcode & (1 << 4)) != 0 {
        //         rn + 12
        //     } else {
        //         rn + 8
        //     }
        // } else {
        //     rn
        // };

        // let rn_v = clir.ins().iconst(I32, rn as i64);

        let result = match operation {
            0b0000 => clir.ins().band(rn_v, op2_v),
            0b0001 => clir.ins().bxor(rn_v, op2_v),
            0b0010 => clir.ins().usub_overflow(rn_v, op2_v).0,
            0b0011 => clir.ins().usub_overflow(op2_v, rn_v).0,
            0b0100 => clir.ins().iadd(rn_v, op2_v),
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
            self.set_reg(clir, rd as i32, result);
        }
    }

    pub fn bl_jit(&mut self, opcode: u32, _jit: &mut JitContext, builder: &mut FunctionBuilder) {
        let offset = (opcode & 0x00FF_FFFF) << 2;
        let link = opcode & (1 << 24) != 0;
        let signed_off = (offset << 6) as i32 >> 6;

        let r15 = self.reg(builder, 15);

        if link {
            let r15_new = builder.ins().iadd_imm(r15, 4);
            let r14_new = builder.ins().band_imm(r15_new, !3);
            
            self.set_reg(builder, 14, r14_new);
        }

        self.branch = true;
        let c15 = ((self.regs[15] + 8).wrapping_add_signed(signed_off)) & !3;

        // TODO: fix signed offset in IR
        let r15_new = {
            // let tmp = builder.ins().iadd_imm(r15, 8);
            builder.ins().iconst(I32, c15 as i64)
            // builder.ins().band_imm(pc_unaligned, !3)
        };

        self.set_reg(builder, 15, r15_new);
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
        jit: &mut JitContext,
        builder: &mut FunctionBuilder
    ) {
        let clir = builder;
        let rn = (opcode as usize & 0x000F_0000) >> 16;
        let rd = (opcode as usize & 0xF000) >> 12;

        let offset = if I {
            clir.ins().iconst(I32, (((opcode & 0xF00) >> 4) | (opcode & 0xF)) as i64)
        } else {
            self.reg(clir, opcode as i32 & 0xF)
        };

        let mut base_with_offset = if U {
            let reg_n = self.reg(clir, rn as i32);
            clir.ins().iadd(reg_n, offset)
        } else {
            let reg_n = self.reg(clir, rn as i32);
            clir.ins().isub(reg_n, offset)
        };

        base_with_offset = clir.ins().iadd_imm(base_with_offset, (rn == 15) as i64 * 8);

        let address = if P { base_with_offset } else { self.reg(clir, rn as i32) };
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
        let jit_bus = clir.ins().iconst(I64, std::ptr::addr_of_mut!(self.bus) as i64);

        if L {
            if !S {
                let mem16 = clir.ins().call(r16_func, &[jit_bus, aligned_addr]);
                let mem16 = {
                    let tmp = clir.inst_results(mem16)[0];
                    clir.ins().rotr(tmp, ror)
                };

                self.set_reg(clir, rd as i32, mem16);
            } else {
                let res = match H {
                    false => {
                        let mem8 = clir.ins().call(r8_func, &[jit_bus, address]);
                        let mem8 = clir.inst_results(mem8)[0];
                        clir.ins().sextend(I8, mem8)
                    }
                    true => {
                        let mem8 = {
                            let mem8 = clir.ins().call(r8_func, &[jit_bus, address]);
                            let mem8 = clir.inst_results(mem8)[0];
                            clir.ins().sextend(I8, mem8)
                        };

                        let mem16 = {
                            let mem16 = clir.ins().call(r16_func, &[jit_bus, address]);
                            let mem16 = clir.inst_results(mem16)[0];
                            clir.ins().sextend(I16, mem16)
                        };

                        clir.ins().select(address_cond, mem8, mem16)
                    }
                };

                self.set_reg(clir, rd as i32, res);
            }
        } else {
            let rdv = self.reg(clir, rd as i32);
            let value = clir.ins().iadd_imm(rdv, if rd == 15 { 12 } else { 0 });
            let value16 = clir.ins().ireduce(I16, value);
            let _ = clir.ins().call(w16_func, &[jit_bus, aligned_addr, value16]);
        }

        self.branch = rd == 15 && L;
        if ((W || !P) && (rn != rd)) || (!L && (W || !P)) {
            self.set_reg(clir, rn as i32, base_with_offset);
        }
    }

    pub fn undefined_jit(&mut self, _opcode: u32, _jit: &mut JitContext, _builder: &mut FunctionBuilder) {
        unimplemented!()
    }

    fn reg(&self, clir: &mut FunctionBuilder, r: i32) -> Value {
        let reg_adr = clir.block_params(clir.current_block().unwrap())[0];
        clir.ins().load(I32, MemFlags::new(), reg_adr, r * 4)
    }

    fn set_reg(&self, clir: &mut FunctionBuilder, r: i32, val: Value) {
        let reg_adr = clir.block_params(clir.current_block().unwrap())[0];
        clir.ins().store(MemFlags::new(), val, reg_adr, r * 4);
    }
}