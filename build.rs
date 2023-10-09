use std::path::Path;

fn main() {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("instructions.rs");

    let prelude = "pub const ARM_INSTRUCTIONS: [fn(&mut Arm7TDMI, u32); 4096] = [\n";
    let mut instrs = String::new();

    // Bits 20-27 and 4-7 are used to index the opcode (2^12 = 4096).
    for i in 0..4096 {
        instrs += &format!("{},\n", decode(i));
    }

    std::fs::write(dest_path, prelude.to_string() + &instrs + "\n];").unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}

fn decode(index: u16) -> String {
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
        format!("Arm7TDMI::swi")
    } else {
        format!("Arm7TDMI::dummy")
    }
}
