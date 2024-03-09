use std::collections::HashMap;

use anyhow::Result;
use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{types::{I16, I32, I8}, AbiParam, Block, InstBuilder, Signature},
        isa::CallConv,
        settings::{self, Configurable},
        Context,
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use paste::paste;

use crate::mmu::{bus::Bus, Mcu};

pub mod arm7tdmi;
pub mod thumb;

macro_rules! link_io_funcs {
    ($module:expr, read, $bits:expr) => {
        paste! {{
            let mut [<sigr $bits>] = $module.make_signature();
            [<sigr $bits>].params.push(AbiParam::new(I32));
            [<sigr $bits>].returns.push(AbiParam::new([<I $bits>]));

            $module.declare_function(concat!("read", $bits), Linkage::Local, &[<sigr $bits>])
        }}
    };
    ($module:expr, write, $bits:expr) => {
        paste! {{
            let mut [<sigw $bits>] = $module.make_signature();
            [<sigw $bits>].params.push(AbiParam::new(I32));
            [<sigw $bits>].params.push(AbiParam::new([<I $bits>]));

            $module.declare_function(concat!("write", $bits), Linkage::Local, &[<sigw $bits>])
        }}
    };
}

/// The basic JIT struct.
pub struct JitContext {
    /// The function builder context, which is reused across multiple
    /// FunctionBuilder instances.
    builder_context: FunctionBuilderContext,

    /// The main Cranelift context, which holds the state for codegen.
    ctx: Context,

    /// The data description, which is to data objects what `ctx` is to functions.
    data_description: DataDescription,

    /// The module, with the jit backend, which manages the JIT'd
    /// functions.
    module: JITModule,
}

impl JitContext {
    pub fn new() -> Result<Self> {
        let mut flag_builder = settings::builder();
        flag_builder.set("is_pic", "true")?;
        flag_builder.set("opt_level", "speed")?;

        let flags = settings::Flags::new(flag_builder);
        let isa = match cranelift_native::builder() {
            Ok(isa_builder) => isa_builder.finish(flags)?,
            Err(err) => panic!("error while looking up target triple: {err}"),
        };

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        // Declare bus read functions.
        builder.symbol("read8", Bus::read8 as *const u8);
        builder.symbol("read16", Bus::read16 as *const u8);
        builder.symbol("read32", Bus::read32 as *const u8);
        // Declare bus write funcitons.
        builder.symbol("write8", Bus::write8 as *const u8);
        builder.symbol("write16", Bus::write16 as *const u8);
        builder.symbol("write32", Bus::write32 as *const u8);

        let module = JITModule::new(builder);

        Ok(Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_description: DataDescription::new(),
            module,
        })
    }

    pub fn create_jit_translator(&mut self) -> Result<JitTranslator<'_>> {
        // Create FuncIDs to be called from the IR later.
        let r8 = link_io_funcs!(self.module, read, 8)?;
        let r16 = link_io_funcs!(self.module, read, 16)?;
        let r32 = link_io_funcs!(self.module, read, 32)?;

        let w8 = link_io_funcs!(self.module, write, 8)?;
        let w16 = link_io_funcs!(self.module, write, 16)?;
        let w32 = link_io_funcs!(self.module, write, 32)?;

        JitTranslator::new(self, &[r8, r16, r32, w8, w16, w32])
    }
}

/// JIT Translator, builds and generates IR.
pub struct JitTranslator<'ctx> {
    /// Function builder to build functions and instructions.
    builder: FunctionBuilder<'ctx>,
    /// Cache instruction blocks based on the program counter (r15).
    blocks: HashMap<u32, (Block, usize)>,
    /// Refer to the JIT Module to access and declare data/funcs.
    module: &'ctx mut JITModule,
    /// SSA representation of GPRs.
    regs: [Variable; 16],
    /// I/O functions to access the bus.
    io: [FuncId; 6],
}

impl<'ctx> JitTranslator<'ctx> {
    pub fn new(jit_ctx: &'ctx mut JitContext, io: &[FuncId]) -> Result<Self> {
        let pointer_type = jit_ctx.module.target_config().pointer_type();

        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(pointer_type));
        sig.returns.push(AbiParam::new(pointer_type));

        let mut builder = FunctionBuilder::new(&mut jit_ctx.ctx.func, &mut jit_ctx.builder_context);

        // Create entry block and seal it immediately.
        let entry_block = builder.create_block();
        builder.seal_block(entry_block);
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        // Initialize all 16 general purpose registers (r0 - r15) as `Variable` within the JIT.
        // Makes them usable as mutable variables while still obeying to SSA.
        let regs = std::array::from_fn(|r| {
            let reg = Variable::new(r);
            let zero = builder.ins().iconst(I32, 0);

            builder.declare_var(reg, I32);
            builder.def_var(reg, zero);

            reg
        });

        Ok(Self {
            builder,
            blocks: HashMap::new(),
            module: &mut jit_ctx.module,
            regs,
            io: io.try_into()?,
        })
    }
}
