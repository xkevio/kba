use std::collections::HashMap;

use anyhow::Result;
use cranelift::{
    prelude::{AbiParam, types::*},
    codegen::settings::{self, Configurable},
};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use paste::paste;

use crate::mmu::{bus::Bus, Mcu};

pub mod arm7tdmi;
pub mod thumb;

#[macro_export]
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
    /// The module, with the jit backend, which manages the JIT'd functions.
    module: JITModule,
    /// Cache instruction blocks based on the program counter (r15).
    blocks: HashMap<u32, (*const u8, usize)>,
    /// I/O functions to access the bus.
    io: [FuncId; 6],
    cmp: bool,
}

impl JitContext {
    pub fn new() -> Result<Self> {
        let mut flag_builder = settings::builder();
        flag_builder.set("is_pic", "false")?;
        flag_builder.set("opt_level", "none")?;

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
        // Declare bus write functions.
        builder.symbol("write8", Bus::write8 as *const u8);
        builder.symbol("write16", Bus::write16 as *const u8);
        builder.symbol("write32", Bus::write32 as *const u8);

        let mut module = JITModule::new(builder);
        let io = JitContext::create_jit_translator(&mut module)?;

        Ok(Self {
            module,
            blocks: HashMap::new(),
            io,
            cmp: false

        })
    }

    fn create_jit_translator(module: &mut JITModule) -> Result<[FuncId; 6]> {
        // Create FuncIDs to be called from the IR later.
        let r8 = link_io_funcs!(module, read, 8)?;
        let r16 = link_io_funcs!(module, read, 16)?;
        let r32 = link_io_funcs!(module, read, 32)?;

        let w8 = link_io_funcs!(module, write, 8)?;
        let w16 = link_io_funcs!(module, write, 16)?;
        let w32 = link_io_funcs!(module, write, 32)?;

        Ok([r8, r16, r32, w8, w16, w32])
    }
}