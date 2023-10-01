use crate::{arm::interpreter::arm7tdmi::Arm7TDMI, mmu::bus::Bus};

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 160;

#[derive(Default)]
pub struct Gba {
    pub bus: Bus,
    pub cpu: Arm7TDMI,

    pub cycles: usize,
    rom: Vec<u8>,
}

impl Gba {
    pub fn with_rom(rom: &[u8]) -> Self {
        Self {
            rom: rom.to_vec(),
            cpu: Arm7TDMI::setup_registers(),
            ..Default::default()
        }
    }

    pub fn run(&mut self) {
        self.cpu.cycle(&mut self.bus);
        self.cycles += 1;
    }
}
