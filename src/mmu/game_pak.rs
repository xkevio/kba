use crate::box_arr;

pub struct GamePak {
    pub rom: Box<[u8; 0x0200_0000]>,
    pub sram: Vec<u8>,
}

impl Default for GamePak {
    fn default() -> Self {
        Self {
            rom: box_arr![0xFF; 0x0200_0000],
            sram: Default::default(),
        }
    }
}
