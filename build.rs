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
    if index & 0b1100_0000_0000 == 0b0000_0000_0000 {
        let imm = index & (1 << 9) != 0;
        let s_bit = index & (1 << 4) != 0;
        format!("data_processing<{}, {}>", imm, s_bit)
    } else if index & 0b0001_1000_1111 == 0b0000_0000_1001 {
        let s_bit = index & (1 << 4) != 0;
        format!("multiply<{}>", s_bit)
    } else if index & 0b0001_1000_1111 == 0b0000_1000_1001 {
        let s_bit = index & (1 << 4) != 0;
        format!("multiply_long<{}>", s_bit)
    } else if index & 0b0001_1000_1111 == 0b0001_0000_1001 {
        // TODO: make B-bit const generic
        format!("swap")
    } else if index & 0b0001_0010_1111 == 0b0001_0010_0001 {
        format!("bx")
    } else {
        todo!()
    }
}
