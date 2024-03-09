//! For the jitted THUMB instructions.

use cranelift::codegen::ir::{types::I64, InstBuilder};

use crate::arm::interpreter::arm7tdmi::Arm7TDMI;

use super::JitTranslator;

impl Arm7TDMI {
    // TODO: less IR for constant extraction.
    pub fn add_sp_jit(&mut self, opcode: u16, jit: &mut JitTranslator) {
        let clir = &mut jit.builder;
        let sub_block = clir.create_block();
        let add_block = clir.create_block();
        
        let opcode = clir.ins().iconst(I64, opcode as i64);

        let offset = {
            let tmp = clir.ins().band_imm(opcode, 0x7F);
            clir.ins().ishl_imm(tmp, 2)
        };
        let sign = clir.ins().band_imm(opcode, 1 << 7);
        let sp = clir.use_var(jit.regs[13]);

        clir.ins().brif(sign, sub_block, &[], add_block, &[]);

        // If sign, subtract offset from SP.
        clir.switch_to_block(sub_block);
        let (res, _) = clir.ins().ssub_overflow(sp, offset);
        clir.def_var(jit.regs[13], res);
        clir.seal_block(sub_block);

        // If not sign, add offset to SP.
        clir.switch_to_block(add_block);
        let (res, _) = clir.ins().sadd_overflow(sp, offset);
        clir.def_var(jit.regs[13], res);
        clir.seal_block(add_block);
    }
}
