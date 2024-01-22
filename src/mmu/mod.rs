pub mod bus;
pub mod dma;
pub mod game_pak;
pub mod irq;
pub mod timer;

/// Create array on the heap, ideally without blowing the stack first.
#[macro_export]
macro_rules! box_arr {
    ($el:expr; $size:expr) => {
        vec![$el; $size].into_boxed_slice().try_into().unwrap()
    };
}

/// Enables range syntax for bit ranges to properly support inclusive end values.
///
/// - `bits!(x, 0..3)` gives bits starting from bit `0` up to bit `3` (exclusive).
/// - `bits!(x, 0..=3)` gives bits starting from bit `0` up to bit `3` (inclusive).
#[macro_export]
macro_rules! bits {
    ($val:expr, $start:literal..$end:literal) => {
        $val.bit_range::<$start, $end>()
    };
    ($val:expr, $start:literal..=$end:literal) => {
        $val.bit_range::<$start, { $end + 1 }>()
    };
}

/// Enables range syntax for setting bit ranges to properly support inclusive end values.
///
/// See `bits!` for range syntax.
#[macro_export]
macro_rules! set_bits {
    ($val:expr, $start:literal..$end:literal, $new_val:expr) => {
        $val = $val.set_bit_range::<$start, $end>($new_val)
    };
    ($val:expr, $start:literal..=$end:literal, $new_val:expr) => {
        $val = $val.set_bit_range::<$start, { $end + 1 }>($new_val)
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

    /// Raw read - meaning no effect on the bus and will also
    /// read non-readable values, just for convenience (I/O).
    fn raw_read16(&mut self, _address: u32) -> u16 {
        unimplemented!()
    }
}
