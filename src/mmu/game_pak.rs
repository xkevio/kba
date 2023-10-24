use super::Mcu;

#[derive(Default)]
pub struct GamePak {
    pub rom: Vec<u8>,
    pub sram: Vec<u8>,
}

impl Mcu for GamePak {
    fn read8(&mut self, address: u32) -> u8 {
        self.rom[address as usize]
    }

    fn write8(&mut self, _address: u32, _value: u8) {
        unimplemented!("No writes to ROM!")
    }
}
