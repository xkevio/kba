use proc_bitfield::bitfield;

pub struct Arm7TDMI {
    pub regs: [u32; 16],
    pub cspr: Cspr,
}

pub enum State {
    Arm,
    Thumb,
}

impl From<bool> for State {
    fn from(value: bool) -> Self {
        match value {
            false => Self::Arm,
            true => Self::Thumb,
        }
    }
}

bitfield! {
    // bits 8-9 arm11 only, 10-23 & 25-26 reserved, 24 unnecessary, 27 armv5 upwards.
    pub struct Cspr(pub u32) {
        pub cspr: u32 @ ..,
        /// Mode bits (fiq, irq, svc, user...)
        pub mode: u8 @ 0..=4,
        /// ARM (0) or THUMB (1)
        pub state: bool [get State] @ 5,
        pub fiq: bool @ 6,
        pub irq: bool @ 7,
        /// Overflow flag
        pub v: bool @ 28,
        /// Carry flag
        pub c: bool @ 29,
        /// Zero flag
        pub z: bool @ 30,
        /// Sign flag
        pub n: bool @ 31,
    }
}

impl Arm7TDMI {
    /// If I is false, operand 2 is a register and gets shifted.
    /// Otherwise, it is an unsigned 8 bit immediate value.
    pub fn barrel_shifter<const I: bool>(&self, op: u16) -> u32 {
        if I {
            ((op & 0xFF) as u32).rotate_right((op as u32 & 0x0F00) * 2)
        } else {
            let rm = self.regs[op as usize & 0xF];
            let shift = op & 0x0FF0;
            todo!()
        }
    }

    pub fn cond<const COND: u8>(&self) -> bool {
        match COND {
            0b0000 => self.cspr.z(),
            0b0001 => !self.cspr.z(),
            0b0010 => self.cspr.c(),
            0b0011 => !self.cspr.c(),
            0b0100 => self.cspr.n(),
            0b0101 => !self.cspr.n(),
            0b0110 => self.cspr.v(),
            0b0111 => !self.cspr.v(),
            0b1000 => self.cspr.c() && !self.cspr.z(),
            0b1001 => self.cspr.c() && self.cspr.z(),
            0b1010 => self.cspr.n() == self.cspr.v(),
            0b1011 => self.cspr.n() != self.cspr.v(),
            0b1100 => !self.cspr.z() && (self.cspr.n() == self.cspr.v()),
            0b1101 => self.cspr.z() || (self.cspr.n() != self.cspr.v()),
            0b1110 => true,
            _ => unreachable!()
        }
    }

    pub fn data_processing<const COND: u8, const I: bool, const S: bool>(&mut self, opcode: u32) {
        let rn = self.regs[opcode as usize & 0x000F_0000];
        let rd = self.regs[opcode as usize & 0xF000];
        let op2 = self.barrel_shifter::<I>(opcode as u16);

        // TODO: carry out from barrel shifter
        if self.cond::<COND>() {
            let result = match (opcode & 0x01E0_0000) >> 21 {
                0b0000 => rn & op2,
                0b0001 => rn ^ op2,
                0b0010 => rn - op2,
                0b0011 => op2 - rn,
                0b0100 => rn + op2,
                0b0101 => rn + op2 + self.cspr.c() as u32,
                0b0110 => rn - op2 + self.cspr.c() as u32 - 1,
                0b0111 => op2 - rn + self.cspr.c() as u32 - 1,
                0b1000 => {
                    if S {
                        let tst = rn & op2;
                        if tst == 0 { self.cspr.set_z(true) }
                        self.cspr.set_c((tst & (1 << 31)) != 0);
                    }

                    rd
                }
                _ => todo!()
            };
        }
    }
}