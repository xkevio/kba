//! For the jitted THUMB instructions.

use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{types::I64, Block, BlockCall, InstBuilder, JumpTable, JumpTableData, ValueListPool},
    },
    frontend::Variable,
};

use crate::arm::interpreter::arm7tdmi::Arm7TDMI;

use super::JitTranslator;

impl Arm7TDMI {
    // pub fn mov_shifted_reg_jit(&mut self, opcode: u16, jit: &mut JitTranslator) {
    //     let clir = &mut jit.builder;

    //     let opcode = clir.ins().iconst(I64, opcode as i64);
    //     let rd = clir.ins().band_imm(opcode, 0x7);
    //     let rs = {
    //         let tmp = clir.ins().ushr_imm(opcode, 3);
    //         clir.ins().band_imm(tmp, 0x7)
    //     };

    //     let offset = {
    //         let tmp = clir.ins().ushr_imm(opcode, 6);
    //         clir.ins().band_imm(tmp, 0x1F)
    //     };
    //     let op = {
    //         let tmp = clir.ins().ushr_imm(opcode, 11);
    //         clir.ins().band_imm(tmp, 0x3)
    //     };

    //     // clir.ins().br_table(op, clir.create_jump_table(
    //     //     JumpTableData::new(
    //     //         BlockCall::new(clir.create_block(), &[], &mut ValueListPool::default()),
    //     //         &[
    //     //             BlockCall::new(clir.create_block(), &[], &mut ValueListPool::default()),
    //     //             BlockCall::new(clir.create_block(), &[], &mut ValueListPool::default()),
    //     //             BlockCall::new(clir.create_block(), &[], &mut ValueListPool::default()),
    //     //         ]
    //     //     )
    //     // ));

    //     // CPSR as register 16 so to speak.
    //     let cpsr = clir.use_var(Variable::new(16));
    //     // let reg_dst = clir.use_var(Variable::)
    //     // clir.ins().brif(/* regs[rd] */, block_then_label, block_then_args, block_else_label, block_else_args)
    // }

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
