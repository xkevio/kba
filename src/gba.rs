// use memmap2::MmapOptions;

use crate::arm::{interpreter::arm7tdmi::Arm7TDMI, jit::JitContext};

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 160;

// #[derive(Default)]
pub struct Gba {
    pub cpu: Arm7TDMI,
    pub cycles: usize,
    rom: Vec<u8>,
}

impl Gba {
    pub fn with_rom(rom: &[u8]) -> Self {
        Self {
            cpu: Arm7TDMI::new(rom),
            rom: rom.to_vec(),
            cycles: 0,
        }
    }

    pub fn run(&mut self, jit: &mut JitContext) {
        if self.cpu.bus.halt && (self.cpu.bus.ie.0 & self.cpu.bus.iff.0) != 0 {
            self.cpu.bus.halt = false;
        }

        let block_cycles = 
            if !self.cpu.bus.halt {
                self.cpu.dispatch_irq();

                // println!("{:X?}", self.cpu.regs);
                let (block, cycles) = self.cpu.get_next_block(jit);
                self.run_block(block);

                cycles
            } else {
                1
            };

        for i in 0..block_cycles { self.cpu.bus.tick(self.cycles + i) }
        self.cycles += block_cycles;
    }

    fn run_block(&mut self, code: *const u8) {
        // let mut buffer = MmapOptions::new()
        //     .len(code.len())
        //     .map_anon()
        //     .unwrap();

        // buffer.copy_from_slice(code);
        // let buffer = buffer.make_exec().unwrap();

        unsafe {
            let code_fn: unsafe extern "C" fn(*mut u32) -> *mut u32 = std::mem::transmute(code);
            let gprs = code_fn(self.cpu.regs.as_mut_ptr());
            let gprs_slice = std::slice::from_raw_parts_mut(gprs, 17);
            self.cpu.regs.copy_from_slice(&gprs_slice);
        }    
    } 
}
