#![allow(dead_code)]
use std::path::Path;

use frontend::SDLApplication;
use gba::Gba;

mod arm;
mod frontend;
mod gba;
mod mmu;
mod ppu;

pub type SdlResult<T> = Result<T, String>;

fn main() -> SdlResult<()> {
    let file_path = std::env::args().nth(1).expect("A rom has to be specified!");
    let file_name = Path::new(&file_path).file_name().unwrap_or_default();
    let rom = std::fs::read(&file_path).map_err(|e| e.to_string())?;

    let mut sdl_application = SDLApplication::new(&format!("κba - {:?}", file_name))?;
    let mut kba = Gba::with_rom(&rom);

    sdl_application.run(&mut kba)
}
