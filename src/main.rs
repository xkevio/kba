#![allow(dead_code)]

use gba::{Gba, LCD_HEIGHT, LCD_WIDTH};
use sdl2::{event::Event, pixels::PixelFormatEnum};

mod arm;
mod gba;
mod mmu;
mod ppu;

type SdlResult<T> = Result<T, String>;

fn main() -> SdlResult<()> {
    let rom_path = std::env::args().nth(1).expect("A rom has to be specified!");
    let rom = std::fs::read(rom_path).map_err(|e| e.to_string())?;

    let mut kba = Gba::with_rom(&rom);
    kba.cpu.bus.game_pak.rom = rom;

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let window = video_subsystem
        .window("κba", LCD_WIDTH as u32, LCD_HEIGHT as u32)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, LCD_WIDTH as u32, LCD_HEIGHT as u32)
        .map_err(|e| e.to_string())?;

    // Actual loop that runs the program and the emulator.
    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                _ => {}
            }
        }

        // For now, update every 266_666 cycles (60 frames).
        while kba.cycles < 266_666 {
            kba.run();
        }

        texture.with_lock(None, |buffer: &mut [u8], _: usize| {
            // bg mode 3
            for (i, px) in kba.cpu.bus.vram[..76800].chunks(2).enumerate() {
                let color555 = u16::from_be_bytes([px[1], px[0]]);
                let [r, g, b, a] = rgb555_to_color(color555).to_be_bytes();

                buffer[i * 4] = r;
                buffer[i * 4 + 1] = g;
                buffer[i * 4 + 2] = b;
                buffer[i * 4 + 3] = a;
            }
        })?;

        kba.cycles = 0;

        canvas.clear();
        canvas.copy(&texture, None, None)?;
        canvas.present();
    }

    Ok(())
}

fn rgb555_to_color(rgb: u16) -> u32 {
    let red = (rgb & 0x1F) as u8;
    let green = ((rgb >> 5) & 0x1F) as u8;
    let blue = ((rgb >> 10) & 0x1F) as u8;

    u32::from_be_bytes([
        (red << 3) | (red >> 2),
        (green << 3) | (green >> 2),
        (blue << 3) | (blue >> 2),
        255,
    ])
}
