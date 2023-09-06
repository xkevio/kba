use gba::{LCD_HEIGHT, LCD_WIDTH};
use sdl2::event::Event;

mod arm;
mod gba;
mod mmu;

type SdlResult<T> = Result<T, String>;

fn main() -> SdlResult<()> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let window = video_subsystem
        .window("κba", LCD_WIDTH as u32, LCD_HEIGHT as u32)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    'main: loop {
        canvas.clear();
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                _ => {}
            }
        }

        canvas.present();
    }

    Ok(())
}
