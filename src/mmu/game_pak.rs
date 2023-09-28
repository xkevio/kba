use super::Mcu;

#[derive(Default)]
pub struct GamePak {}

impl Mcu for GamePak {
    fn read8(&mut self, address: u32) -> u8 {
        todo!()
    }

    fn write8(&mut self, address: u32, value: u8) {
        todo!()
    }
}
