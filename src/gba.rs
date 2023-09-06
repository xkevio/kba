use crate::{arm::interpreter::arm7tdmi::Arm7TDMI, mmu::bus::Bus};

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 160;

pub struct Gba {
    pub bus: Bus,
    pub cpu: Arm7TDMI,
    rom: Vec<u8>,
}

impl Gba {
    pub fn run(&self) {
        todo!()
    }
}
