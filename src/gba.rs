use crate::arm::{interpreter::arm7tdmi::Arm7TDMI, jit::JitTranslator};

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 160;

// #[derive(Default)]
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
            cycles: 0,
        }
    }

    pub fn run(&mut self, jit_translator: &mut JitTranslator) {
        if self.cpu.bus.halt && (self.cpu.bus.ie.0 & self.cpu.bus.iff.0) != 0 {
            self.cpu.bus.halt = false;
        }

        if !self.cpu.bus.halt {
            self.cpu.dispatch_irq();
            // self.cpu.cycle();

            let (block, cycles) = self.cpu.get_next_block(jit_translator);
            // todo: execute block and adjust cycles.
        }

        self.cpu.bus.tick(self.cycles);
        self.cycles += 1;
    }
}
