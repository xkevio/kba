use crate::arm::interpreter::arm7tdmi::Arm7TDMI;

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 160;

#[derive(Default)]
pub struct Gba {
    pub cpu: Arm7TDMI,
    pub cycles: usize,
    rom: Vec<u8>,
}

impl Gba {
    pub fn with_rom(rom: &[u8]) -> Self {
        Self {
            cpu: Arm7TDMI::new(rom),
            rom: rom.to_vec(),
            ..Default::default()
        }
    }

    pub fn run(&mut self) {
        if self.cpu.bus.halt && (self.cpu.bus.ie.0 & self.cpu.bus.iff.0) != 0 {
            self.cpu.bus.halt = false;
        }

        if !self.cpu.bus.halt {
            self.cpu.dispatch_irq();
            self.cpu.cycle();
        }

        // Later refactor into bus cycle() method.
        self.cpu.bus.ppu.cycle(
            &*self.cpu.bus.vram,
            &self.cpu.bus.palette_ram,
            &mut self.cpu.bus.iff,
        );
        self.cycles += 1;
    }
}
