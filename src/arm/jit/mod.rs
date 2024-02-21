use std::collections::HashMap;

use anyhow::Result;
use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{
            types::{I16, I8},
            AbiParam, Block, Function, InstBuilder, Signature, UserFuncName, Value,
        },
        isa::{lookup, CallConv},
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
};
use target_lexicon::Triple;

pub mod thumb;

pub struct JitFunctionContext {
    func: Function,
    func_ctx: FunctionBuilderContext,
}

impl JitFunctionContext {
    pub fn new(sig: Signature) -> Self {
        Self {
            func: Function::with_name_signature(UserFuncName::user(0, 0), sig),
            func_ctx: FunctionBuilderContext::new(),
        }
    }
}

/// The basic JIT struct.
pub struct Jit<'a> {
    /// Function builder to build functions and instructions.
    builder: FunctionBuilder<'a>,
    /// Cache instruction blocks based on the program counter (r15).
    blocks: HashMap<u32, Vec<Block>>,
}

impl<'a> Jit<'a> {
    pub fn new(jit_ctx: &'a mut JitFunctionContext) -> Result<Self> {
        let mut builder = settings::builder();
        builder.set("opt_level", "speed")?;

        let flags = settings::Flags::new(builder);
        let isa = match lookup(Triple::host()) {
            Ok(isa) => isa.finish(flags)?,
            Err(err) => panic!("error while looking up target triple: {err}"),
        };

        let pointer_type = isa.pointer_type();
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(pointer_type));
        sig.returns.push(AbiParam::new(pointer_type));

        let mut builder = FunctionBuilder::new(&mut jit_ctx.func, &mut jit_ctx.func_ctx);

        let pointer = Variable::new(0);
        builder.declare_var(pointer, pointer_type);

        let block = builder.create_block();
        builder.seal_block(block);
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);

        let _memory_addr = builder.block_params(block)[0];
        let zero = builder.ins().iconst(pointer_type, 0);
        builder.def_var(pointer, zero);

        Ok(Self {
            builder,
            blocks: HashMap::new(),
        })
    }

    /// Format 12: load address. (TODO: this is a JIT test.)
    pub fn load_addr<const SP: bool>(&mut self, opcode: u16) {
        let offset = opcode as u8 as u32;
        let jit_offset = Variable::new(1);
        let jit_offset_val = self.builder.ins().iconst(I8, offset as i64);

        self.builder.declare_var(jit_offset, I8);
        self.builder.def_var(jit_offset, jit_offset_val);

        let rd = self.builder.ins().ushr_imm(jit_offset_val, 8);
        let rd = self.builder.ins().band_imm(rd, 0x7);
        // let rd = (opcode as usize >> 8) & 0x7;

        // self.regs[rd] = match SP {
        //     false => ((self.regs[15] + 4) & !2) + (offset << 2),
        //     true => self.regs[13] + (offset << 2),
        // };
    }
}
