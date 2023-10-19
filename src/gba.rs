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
            rom: rom.to_vec(),
            cpu: Arm7TDMI::setup_registers(false),
            ..Default::default()
        }
    }

    pub fn run(&mut self) {
        self.cpu.cycle();
        self.cpu
            .bus
            .io
            .ppu
            .cycle(&*self.cpu.bus.vram, &self.cpu.bus.palette_ram);

        self.cycles += 1;
    }
}
