use std::collections::HashMap;

use anyhow::Result;
use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{types::I32, AbiParam, Block, Function, InstBuilder, Signature, UserFuncName},
        isa::{lookup, CallConv},
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
};
use target_lexicon::Triple;

use crate::mmu::bus::Bus;

pub mod arm7tdmi;
pub mod thumb;

/// Saved Program Status Register as an alias for differentiation. Same structure as CPSR.
// type Spsr = Cpsr;
/// Each mode has its own banked registers (mostly r13 and r14).
// #[derive(Default, Clone, Copy)]
// struct BankedRegisters {
//     spsr: Spsr,
//     bank: [u32; 7],
// }

// /// Initialize `BankedRegister` with SPSR and SP while filling the rest.
// macro_rules! bank {
//     (spsr: $spsr:expr, sp: $sp:expr) => {
//         BankedRegisters {
//             spsr: $spsr,
//             bank: arr_with(5, $sp),
//         }
//     };
// }

// Include the generated LUT at compile time.
// include!(concat!(env!("OUT_DIR"), "/arm_instructions.rs"));
// include!(concat!(env!("OUT_DIR"), "/thumb_instructions.rs"));

// #[derive(Default)]
pub struct Arm7TDMI<'a> {
    /// 16 registers, most GPR, r14 = LR, r15 = PC.
    pub regs: [Variable; 16],
    /// Current Program Status Register.
    // pub cpsr: Cpsr,

    /// The memory bus, owned by the CPU for now.
    pub bus: Bus,
    pub jit: Jit<'a>,

    /// Saved Program Status Register for all modes but User.
    // spsr: Spsr,
    /// The other banked registers of the other modes.
    // banked_regs: Registers,

    /// If the prev. instruction directly **set** r15.
    pub(super) branch: bool,
}

#[derive(PartialEq)]
pub enum State {
    Arm,
    Thumb,
}

// --- JIT INITIALIZATION --- //

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

        let block = builder.create_block();
        builder.seal_block(block);
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);

        Ok(Self {
            builder,
            blocks: HashMap::new(),
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
