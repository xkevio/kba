//! For the jitted ARM32 instructions.

use cranelift::codegen::ir::Block;

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
}