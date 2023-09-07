use std::path::Path;

fn main() {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("instructions.rs");

    let prelude = "pub const INSTRUCTIONS: [fn(&mut Arm7TDMI); 4096] = [\n";
    let mut instrs = String::new();

    // Bits 20-27 and 4-7 are used to index the opcode (2^12 = 4096).
    for _ in 0..4096 {
        // TODO: insert fn pointers.
        // instrs += &format!("{},\n", ...);
    }

    std::fs::write(dest_path, prelude.to_string() + &instrs + "\n];").unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
