pub mod interpreter;

/// Set V (overflow) flag and save repetition.
#[macro_export]
macro_rules! ov {
    ($res:expr, $opcode:expr, $self:ident) => {{
        let (res, ov) = $res;

        // If S-bit is set and if rd != r15.
        if S && (($opcode as usize & 0xF000) >> 12) != 15 {
            if ov {
                $self.cpsr.set_v(ov);
            }
        }

        res
    }};
}
