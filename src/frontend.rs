use paste::paste;
use sdl2::{
    event::Event,
    keyboard::Scancode,
    pixels::PixelFormatEnum,
    render::{Canvas, Texture, TextureCreator},
    video::{Window, WindowContext},
    EventPump,
};

use crate::{
    gba::{Gba, LCD_HEIGHT, LCD_WIDTH},
    ppu, SdlResult,
};

macro_rules! process_scancodes {
    ($kba:expr, $state:expr; $($name:ident => $code:ident),*) => {
        paste! {
            $(
                if $state.is_scancode_pressed(Scancode::$code) {
                    $kba.cpu.bus.key_input.[<set_ $name>](false);
                }
            )*
        }
    };
}

pub struct SDLApplication {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
}

impl SDLApplication {
    pub fn new(title: &str) -> SdlResult<Self> {
        let sdl_context = sdl2::init()?;
        let video_subsystem = sdl_context.video()?;

        let window = video_subsystem
            .window(title, LCD_WIDTH as u32 * 2, LCD_HEIGHT as u32 * 2)
            .position_centered()
            .build()
            .map_err(|e| e.to_string())?;

        let event_pump = sdl_context.event_pump()?;
        let canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        let texture_creator = canvas.texture_creator();

        Ok(Self {
            event_pump,
            canvas,
            texture_creator,
        })
    }

    pub fn run(&mut self, kba: &mut Gba) -> SdlResult<()> {
        // TODO.
        let _jit_translator = kba
            .cpu
            .jit_ctx
            .create_jit_translator()
            .expect("Failed to initialize JIT.");

        let mut texture = self
            .texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, LCD_WIDTH as u32, LCD_HEIGHT as u32)
            .map_err(|e| e.to_string())?;

        'main: loop {
            for event in self.event_pump.poll_iter() {
                if let Event::Quit { .. } = event {
                    break 'main;
                }
            }

            let keyboard_state = self.event_pump.keyboard_state();
            process_scancodes!(kba, keyboard_state;
                up => Up,
                left => Left,
                down => Down,
                right => Right,
                start => Return,
                select => Backspace,
                a => X,
                b => Z,
                l => A,
                r => S
            );

            // todo: vsync delay / sleep.
            // For now, update every 266_666 cycles (60 frames).
            while kba.cycles < 266_666 {
                // TODO: pass `jit` to opcode functions (change signature for JIT LUT).
                kba.run();
            }

            // Update frame and convert Option pixel values to corresponding colors.
            // Needs backdrop color which is always color 0 of pal 0 for ignored pixels.
            self.update_texture(
                &mut texture,
                &kba.cpu.bus.ppu.buffer[0..(LCD_WIDTH * LCD_HEIGHT)],
                u16::from_le_bytes([kba.cpu.bus.palette_ram[0], kba.cpu.bus.palette_ram[1]]),
            )?;

            kba.cycles = 0;
            kba.cpu.bus.key_input.set_keyinput(0x03FF);

            self.canvas.clear();
            self.canvas.copy(&texture, None, None)?;
            self.canvas.present();
        }

        Ok(())
    }

    fn update_texture(
        &self,
        texture: &mut Texture,
        buffer: &[Option<u16>],
        backdrop: u16,
    ) -> SdlResult<()> {
        texture.with_lock(None, |buf: &mut [u8], _: usize| {
            for (i, px) in buffer[0..(LCD_WIDTH * LCD_HEIGHT)].iter().enumerate() {
                let [r, g, b, a] = match px {
                    Some(color) => ppu::rgb555_to_color(*color).to_be_bytes(),
                    None => ppu::rgb555_to_color(backdrop).to_be_bytes(),
                };

                buf[i * 4] = r;
                buf[i * 4 + 1] = g;
                buf[i * 4 + 2] = b;
                buf[i * 4 + 3] = a;
            }
        })
    }
}
