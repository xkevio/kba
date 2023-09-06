use std::path::Path;

fn main() {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("instructions.rs");

    let prelude = "pub const INSTRUCTIONS: [u32; 4096] = [";
    let mut instrs = String::new();

    // Bits 20-27 and 4-7 are used to index the opcode (2^12 = 4096).
    for _ in 0..4096 {
        instrs += &format!("{}", 0xFFFF_FFFFu32 & 0x0FF0_00F0);
    }

    std::fs::write(&dest_path, prelude.to_string() + &instrs).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}