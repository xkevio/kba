use num_traits::Unsigned;

pub mod bus;
pub mod game_pak;
pub mod io;

pub trait Mcu {
    fn read<T: Unsigned>(&mut self, address: u32) -> T;
    fn write<T: Unsigned>(&mut self, address: u32, value: T);
}
