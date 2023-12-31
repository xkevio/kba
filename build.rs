#![allow(clippy::useless_format)]
use std::{error::Error, path::Path};

/// This build script generates two look up tables at build time,
/// which are then included in the actual code.
///
/// These function pointer LUTs can then be indexed with certain bits
/// of the opcode encoding. This code generation ensures less manual work.
fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

    // Define the output files and const array signatures of the function pointer LUTs.
    let arm_path = Path::new(&out_dir).join("arm_instructions.rs");
    let thumb_path = Path::new(&out_dir).join("thumb_instructions.rs");

    let arm_pre = "pub const ARM_INSTRUCTIONS: [fn(&mut Arm7TDMI, u32); 4096] = [\n";
    let thumb_pre = "pub const THUMB_INSTRUCTIONS: [fn(&mut Arm7TDMI, u16); 256] = [\n";

    let mut arm_instrs = String::new();
    let mut thumb_instrs = String::new();

    // Bits 20-27 and 4-7 are used to index the opcode (2^12 = 4096).
    for i in 0..4096 {
        arm_instrs += &format!("{},\n", decode_arm(i));
    }

    // Upper 8 bits are used to index the opcode (2^8 = 256).
    for i in 0..=255 {
        thumb_instrs += &format!("{},\n", decode_thumb(i));
    }

    std::fs::write(arm_path, arm_pre.to_string() + &arm_instrs + "\n];")?;
    std::fs::write(thumb_path, thumb_pre.to_string() + &thumb_instrs + "\n];")?;
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

/// Decode an ARMv4 opcode based on 12 bits (20-27 and 4-7) with bitmasks.
fn decode_arm(index: u16) -> String {
    if index & 0b1111_1100_1111 == 0b0000_0000_1001 {
        let s_bit = index & (1 << 4) != 0;
        format!("Arm7TDMI::multiply::<{}>", s_bit)
    } else if index & 0b1111_1000_1111 == 0b0000_1000_1001 {
        let s_bit = index & (1 << 4) != 0;
        format!("Arm7TDMI::multiply_long::<{}>", s_bit)
    } else if index & 0b1111_1011_1111 == 0b0001_0000_1001 {
        let b_bit = index & (1 << 6) != 0;
        format!("Arm7TDMI::swap::<{}>", b_bit)
    } else if index & 0b1111_1111_1111 == 0b0001_0010_0001 {
        format!("Arm7TDMI::bx")
    } else if index & 0b1110_0000_0000 == 0b1010_0000_0000 {
        format!("Arm7TDMI::bl")
    } else if index & 0b1110_0100_1001 == 0b0000_0000_1001 {
        let p_bit = index & (1 << 8) != 0;
        let u_bit = index & (1 << 7) != 0;
        let w_bit = index & (1 << 5) != 0;
        let l_bit = index & (1 << 4) != 0;
        let s_bit = index & (1 << 2) != 0;
        let h_bit = index & (1 << 1) != 0;

        format!(
            "Arm7TDMI::hw_signed_data_transfer::<false, {}, {}, {}, {}, {}, {}>",
            p_bit, u_bit, w_bit, l_bit, s_bit, h_bit
        )
    } else if index & 0b1110_0100_1001 == 0b0000_0100_1001 {
        let p_bit = index & (1 << 8) != 0;
        let u_bit = index & (1 << 7) != 0;
        let w_bit = index & (1 << 5) != 0;
        let l_bit = index & (1 << 4) != 0;
        let s_bit = index & (1 << 2) != 0;
        let h_bit = index & (1 << 1) != 0;

        format!(
            "Arm7TDMI::hw_signed_data_transfer::<true, {}, {}, {}, {}, {}, {}>",
            p_bit, u_bit, w_bit, l_bit, s_bit, h_bit
        )
    } else if index & 0b1110_0000_0000 == 0b1000_0000_0000 {
        let p_bit = index & (1 << 8) != 0;
        let u_bit = index & (1 << 7) != 0;
        let s_bit = index & (1 << 6) != 0;
        let w_bit = index & (1 << 5) != 0;
        let l_bit = index & (1 << 4) != 0;

        format!(
            "Arm7TDMI::block_data_transfer::<{}, {}, {}, {}, {}>",
            p_bit, u_bit, s_bit, w_bit, l_bit
        )
    } else if index & 0b1101_1001_0000 == 0b0001_0000_0000 {
        let imm = index & (1 << 9) != 0;
        let psr = index & (1 << 6) != 0;
        format!("Arm7TDMI::psr_transfer::<{}, {}>", imm, psr)
    } else if index & 0b1100_0000_0000 == 0b0000_0000_0000 {
        let imm = index & (1 << 9) != 0;
        let s_bit = index & (1 << 4) != 0;
        format!("Arm7TDMI::data_processing::<{}, {}>", imm, s_bit)
    } else if index & 0b1100_0000_0000 == 0b0100_0000_0000 {
        let i_bit = index & (1 << 9) != 0;
        let p_bit = index & (1 << 8) != 0;
        let u_bit = index & (1 << 7) != 0;
        let b_bit = index & (1 << 6) != 0;
        let w_bit = index & (1 << 5) != 0;
        let l_bit = index & (1 << 4) != 0;

        format!(
            "Arm7TDMI::single_data_transfer::<{}, {}, {}, {}, {}, {}>",
            i_bit, p_bit, u_bit, b_bit, w_bit, l_bit
        )
    } else if index & 0b1111_0000_0000 == 0b1111_0000_0000 {
        format!("Arm7TDMI::swi::<false>")
    } else {
        format!("Arm7TDMI::undefined")
    }
}

/// Decode a THUMB opcode based on 8 bits (8-15) with bitmasks.
fn decode_thumb(index: u8) -> String {
    if index & 0b1111_1000 == 0b0001_1000 {
        let imm = index & (1 << 2) != 0;
        format!("Arm7TDMI::add_sub::<{}>", imm)
    } else if index & 0b1110_0000 == 0b0000_0000 {
        format!("Arm7TDMI::mov_shifted_reg")
    } else if index & 0b1110_0000 == 0b0010_0000 {
        format!("Arm7TDMI::mov_cmp_alu_imm")
    } else if index & 0b1111_1100 == 0b0100_0000 {
        format!("Arm7TDMI::alu_ops")
    } else if index & 0b1111_1100 == 0b0100_0100 {
        format!("Arm7TDMI::hi_reg_op_bx")
    } else if index & 0b1111_1000 == 0b0100_1000 {
        format!("Arm7TDMI::pc_rel_load")
    } else if index & 0b1111_0010 == 0b0101_0000 {
        let l_bit = index & (1 << 3) != 0;
        let b_bit = index & (1 << 2) != 0;

        format!("Arm7TDMI::load_store_reg::<{}, {}>", l_bit, b_bit)
    } else if index & 0b1111_0010 == 0b0101_0010 {
        let h_bit = index & (1 << 3) != 0;
        let s_bit = index & (1 << 2) != 0;

        format!("Arm7TDMI::load_store_hw_signext::<{}, {}>", h_bit, s_bit)
    } else if index & 0b1110_0000 == 0b0110_0000 {
        let l_bit = index & (1 << 3) != 0;
        let b_bit = index & (1 << 4) != 0;

        format!("Arm7TDMI::load_store_imm::<{}, {}>", l_bit, b_bit)
    } else if index & 0b1111_0000 == 0b1000_0000 {
        let l_bit = index & (1 << 3) != 0;
        format!("Arm7TDMI::load_store_hw::<{}>", l_bit)
    } else if index & 0b1111_0000 == 0b1001_0000 {
        let l_bit = index & (1 << 3) != 0;
        format!("Arm7TDMI::sp_rel_load_store::<{}>", l_bit)
    } else if index & 0b1111_0000 == 0b1010_0000 {
        let sp = index & (1 << 3) != 0;
        format!("Arm7TDMI::load_addr::<{}>", sp)
    } else if index & 0b1111_1111 == 0b1011_0000 {
        format!("Arm7TDMI::add_sp")
    } else if index & 0b1111_0110 == 0b1011_0100 {
        let l_bit = index & (1 << 3) != 0;
        let r_bit = index & 1 != 0;

        format!("Arm7TDMI::push_pop::<{}, {}>", l_bit, r_bit)
    } else if index & 0b1111_0000 == 0b1100_0000 {
        let l_bit = index & (1 << 3) != 0;
        format!("Arm7TDMI::ldm_stm::<{}>", l_bit)
    } else if index & 0b1111_1111 == 0b1101_1111 {
        format!("Arm7TDMI::t_swi")
    } else if index & 0b1111_0000 == 0b1101_0000 {
        format!("Arm7TDMI::cond_branch")
    } else if index & 0b1111_0000 == 0b1110_0000 {
        format!("Arm7TDMI::branch")
    } else if index & 0b1111_0000 == 0b1111_0000 {
        let h_bit = index & (1 << 3) != 0;
        format!("Arm7TDMI::long_branch::<{}>", h_bit)
    } else {
        format!("Arm7TDMI::t_undefined")
    }
}
