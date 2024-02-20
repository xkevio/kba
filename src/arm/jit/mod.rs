use std::collections::HashMap;

use anyhow::Result;
use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{AbiParam, Function, Signature, UserFuncName},
        isa::{lookup, Builder, CallConv},
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
};
use target_lexicon::Triple;

pub mod thumb;

pub struct Jit {
    builder: Builder,
    blocks: HashMap<u32, Vec<u8>>,
}

impl Jit {
    pub fn new() -> Result<Self> {
        let mut builder = settings::builder();
        builder.set("opt_level", "speed")?;

        let flags = settings::Flags::new(builder);
        let isa = match lookup(Triple::host()) {
            Ok(isa) => isa.finish(flags)?,
            Err(err) => panic!("error whie looking up target triple: {err}"),
        };

        let pointer_type = isa.pointer_type();
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(pointer_type));
        sig.returns.push(AbiParam::new(pointer_type));

        let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut func_ctx);

        let pointer = Variable::new(0);
        builder.declare_var(pointer, pointer_type);

        todo!()
    }
}
