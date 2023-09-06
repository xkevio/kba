use num_traits::Unsigned;

pub mod bus;
pub mod game_pak;
pub mod io;

trait Mcu {
    fn read<T: Unsigned>(address: u32) -> T;
    fn write<T: Unsigned>(address: u32, value: T);
}
