use super::Mcu;

#[derive(Default)]
pub struct Io {}

impl Mcu for Io {
    fn read8(&mut self, _address: u32) -> u8 {
        todo!()
    }

    fn write8(&mut self, _address: u32, _value: u8) {
        todo!()
    }
}
