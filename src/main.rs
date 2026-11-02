#![allow(dead_code)]
use std::path::PathBuf;

use anyhow::Result;
use bpaf::Bpaf;
use frontend::SDLApplication;
use gba::Gba;

mod arm;
mod frontend;
mod gba;
mod mmu;
mod ppu;

/// kba: Gameboy Advance Emulator
#[derive(Debug, Clone, Bpaf)]
#[bpaf(options)]
struct KbaCliOptions {
    /// A path to a GBA-compatible ROM file.
    #[bpaf(short, long)]
    rom: PathBuf,
    /// A path to the GBA BIOS ROM (necessary for most games).
    #[bpaf(short, long)]
    bios: PathBuf,
    /// Whether to skip the BIOS boot sequence.
    #[bpaf(short, long)]
    skip_bios: bool,
}

fn main() -> Result<()> {
    let KbaCliOptions {
        rom,
        bios,
        skip_bios,
    } = kba_cli_options().run();

    let mut sdl_application = SDLApplication::new(&format!(
        "κba - {:?}",
        rom.file_name().unwrap().to_str().unwrap()
    ))?;

    let rom = std::fs::read(&rom)?;
    let bios = std::fs::read(bios)?;
    let mut kba = Gba::new(&rom, &bios, skip_bios);

    sdl_application.run(&mut kba)
}
