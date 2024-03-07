use std::collections::HashMap;

use anyhow::Result;
use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{types::I32, AbiParam, Block, InstBuilder, Signature},
        isa::CallConv,
        settings::{self, Configurable},
        Context,
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Module};

pub mod arm7tdmi;
pub mod thumb;

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

        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);

        Ok(Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_description: DataDescription::new(),
            module,
        })
    }

    pub fn create_jit_translator(&mut self) -> Result<JitTranslator<'_>> {
        JitTranslator::new(self)
    }
}

/// JIT Translator, builds and generates IR.
pub struct JitTranslator<'ctx> {
    module: &'ctx mut JITModule,
    /// Function builder to build functions and instructions.
    builder: FunctionBuilder<'ctx>,
    /// Cache instruction blocks based on the program counter (r15).
    blocks: HashMap<u32, Vec<Block>>,
}

impl<'ctx> JitTranslator<'ctx> {
    pub fn new(jit_ctx: &'ctx mut JitContext) -> Result<Self> {
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

        Ok(Self {
            builder,
            blocks: HashMap::new(),
            module: &mut jit_ctx.module,
        })
    }

    /// Initialize all 16 general purpose registers (r0 - r15) as `Variable` within the JIT.
    ///
    /// Makes them usable as mutable variables while still obeying to SSA.
    fn initialize_gprs(&mut self) -> [Variable; 16] {
        std::array::from_fn(|r| {
            let reg = Variable::new(r);
            let zero = self.builder.ins().iconst(I32, 0);

            self.builder.declare_var(reg, I32);
            self.builder.def_var(reg, zero);

            reg
        })
    }
}
