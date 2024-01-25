use super::Mcu;
use proc_bitfield::ConvRaw;
use std::ops::{Index, IndexMut};

#[derive(Default, Clone, Copy)]
pub struct DMAChannels([DMA; 4]);

impl Mcu for DMAChannels {
    fn read16(&mut self, address: u32) -> u16 {
        match address {
            0x00BA => u16::from(self[0]),
            0x00C6 => u16::from(self[1]),
            0x00D2 => u16::from(self[2]),
            0x00DE => u16::from(self[3]),
            _ => 0,
        }
    }

    fn read8(&mut self, address: u32) -> u8 {
        match address & 1 == 0 {
            true => self.read16(address) as u8,
            false => (self.read16(address & !1) >> 8) as u8,
        }
    }

    fn write16(&mut self, address: u32, value: u16) {
        match address {
            // Assign the DMA source address, 27 bit (0-2) and 28 bit for 3.
            0x00B0 => self[0].src = value as u32,
            0x00B2 => self[0].src |= (value as u32 & 0x7FF) << 16,
            0x00BC => self[1].src = value as u32,
            0x00BE => self[1].src |= (value as u32 & 0x7FF) << 16,
            0x00C8 => self[2].src = value as u32,
            0x00CA => self[2].src |= (value as u32 & 0x7FF) << 16,
            0x00D4 => self[3].src = value as u32,
            0x00D6 => self[3].src |= (value as u32 & 0xFFF) << 16,

            // Assign the DMA destination address, 27 bit (0-2) and 28 bit for 3.
            0x00B4 => self[0].dst = value as u32,
            0x00B6 => self[0].dst |= (value as u32 & 0x7FF) << 16,
            0x00C0 => self[1].dst = value as u32,
            0x00C2 => self[1].dst |= (value as u32 & 0x7FF) << 16,
            0x00CC => self[2].dst = value as u32,
            0x00CE => self[2].dst |= (value as u32 & 0x7FF) << 16,
            0x00D8 => self[3].dst = value as u32,
            0x00DA => self[3].dst |= (value as u32 & 0xFFF) << 16,

            // Assign DMA word count units, 14 bit (0-2) and 16 bit for 3.
            0x00B8 => self[0].word_count = value & 0x3FFF,
            0x00C4 => self[1].word_count = value & 0x3FFF,
            0x00D0 => self[2].word_count = value & 0x3FFF,
            0x00DC => self[3].word_count = value,

            // Update the DMA channel attributes via the control register.
            0x00BA => self[0].apply_dma_cnt(value),
            0x00C6 => self[1].apply_dma_cnt(value),
            0x00D2 => self[2].apply_dma_cnt(value),
            0x00DE => self[3].apply_dma_cnt(value),
            _ => {}
        }
    }

    fn write8(&mut self, address: u32, value: u8) {
        let [lo, hi] = self.raw_read16(address & !1).to_le_bytes();
        match address & 1 == 0 {
            true => self.write16(address, (hi as u16) << 8 | value as u16),
            false => self.write16(address & !1, (value as u16) << 8 | lo as u16),
        }
    }

    fn raw_read16(&mut self, _address: u32) -> u16 {
        match _address {
            0x00B0 => self[0].src as u16,
            0x00B2 => (self[0].src >> 16) as u16,
            0x00BC => self[1].src as u16,
            0x00BE => (self[1].src >> 16) as u16,
            0x00C8 => self[2].src as u16,
            0x00CA => (self[2].src >> 16) as u16,
            0x00D4 => self[3].src as u16,
            0x00D6 => (self[3].src >> 16) as u16,

            0x00B4 => self[0].dst as u16,
            0x00B6 => (self[0].dst >> 16) as u16,
            0x00C0 => self[1].dst as u16,
            0x00C2 => (self[1].dst >> 16) as u16,
            0x00CC => self[2].dst as u16,
            0x00CE => (self[2].dst >> 16) as u16,
            0x00D8 => self[3].dst as u16,
            0x00DA => (self[3].dst >> 16) as u16,

            0x00B8 => self[0].word_count,
            0x00C4 => self[1].word_count,
            0x00D0 => self[2].word_count,
            0x00DC => self[3].word_count,

            0x00BA | 0x00C6 | 0x00D2 | 0x00DE => self.read16(_address),
            _ => 0
        }
    }
}

impl Index<usize> for DMAChannels {
    type Output = DMA;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for DMAChannels {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

#[derive(Default, Clone, Copy)]
pub struct DMA {
    pub src: u32,
    pub dst: u32,
    pub word_count: u16,

    pub src_addr_ctrl: AddrControl,
    pub dst_addr_ctrl: AddrControl,
    pub start_timing: StartTiming,

    pub repeat: bool,
    pub transfer_type: bool,
    pub pak_drq: bool,
    pub dma_irq: bool,
    pub enable: bool,
}

impl DMA {
    /// Update all the bits from the DMAxCNT_H register.
    fn apply_dma_cnt(&mut self, value: u16) {
        self.dst_addr_ctrl = AddrControl::try_from((value & 0x60) >> 5).unwrap();
        self.src_addr_ctrl = AddrControl::try_from((value & 0x110) >> 7).unwrap();
        self.start_timing = StartTiming::try_from((value & 0x3000) >> 12).unwrap();

        self.repeat = value & (1 << 9) != 0;
        self.transfer_type = value & (1 << 10) != 0;
        self.pak_drq = value & (1 << 11) != 0;
        self.dma_irq = value & (1 << 14) != 0;
        self.enable = value & (1 << 15) != 0;
    }
}

impl From<DMA> for u16 {
    /// Convert DMA struct into DMAxCNT_H register.
    fn from(value: DMA) -> Self {
        (value.enable as u16) << 15
            | (value.dma_irq as u16) << 14
            | (value.start_timing as u16) << 12
            | (value.pak_drq as u16) << 11
            | (value.transfer_type as u16) << 10
            | (value.repeat as u16) << 9
            | (value.src_addr_ctrl as u16) << 7
            | (value.dst_addr_ctrl as u16) << 5
    }
}

#[derive(ConvRaw, Default, Clone, Copy, PartialEq)]
pub enum AddrControl {
    #[default]
    Increment,
    Decrement,
    Fixed,
    IncReload,
}

#[derive(ConvRaw, Default, Clone, Copy, PartialEq)]
pub enum StartTiming {
    #[default]
    Immediate,
    VBlank,
    HBlank,
    Special,
}
