pub mod bus;
pub mod game_pak;
pub mod io;
pub mod irq;

/// Create array on the heap, ideally without blowing the stack first.
#[macro_export]
macro_rules! box_arr {
    ($el:expr; $size:expr) => {
        vec![$el; $size].into_boxed_slice().try_into().unwrap()
    };
}

pub trait Mcu {
    fn read32(&mut self, address: u32) -> u32 {
        u32::from_le_bytes([
            self.read8(address),
            self.read8(address + 1),
            self.read8(address + 2),
            self.read8(address + 3),
        ])
    }

    fn write32(&mut self, address: u32, value: u32) {
        let [a, b, c, d] = value.to_le_bytes();

        self.write8(address, a);
        self.write8(address + 1, b);
        self.write8(address + 2, c);
        self.write8(address + 3, d);
    }

    fn read16(&mut self, address: u32) -> u16 {
        u16::from_le_bytes([self.read8(address), self.read8(address + 1)])
    }
    fn write16(&mut self, address: u32, value: u16) {
        let [a, b] = value.to_le_bytes();

        self.write8(address, a);
        self.write8(address + 1, b);
    }

    fn read8(&mut self, address: u32) -> u8;
    fn write8(&mut self, address: u32, value: u8);
}
