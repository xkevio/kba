pub mod interpreter;

/// Set V (overflow) and C (carry) flag and save repetition.
///
/// - Set C to carry out of bit31 in ALU.
/// - Set (signed) overflow -- check sign bits of operands and result.
#[macro_export]
macro_rules! fl {
    // ADD, ADC, CMN
    ($a:expr, $b:expr, +, $self:ident, $cpsr:ident) => {{
        let (res, ov) = $a.overflowing_add($b);

        $self.$cpsr.set_c(ov);
        $self
            .$cpsr
            .set_v((($a >> 31) == ($b >> 31)) && (($a >> 31) != (res >> 31)));

        res
    }};

    // SUB, RSB
    ($a:expr, $b:expr, -, $self:ident, $cpsr:ident) => {{
        let res = $a - $b;

        $self.$cpsr.set_c($a >= $b);
        $self.$cpsr.set_v(((($a ^ $b) & ($a ^ res)) >> 31) != 0);

        res
    }};

    // SBC, RSC
    ($a:expr, $b:expr, $c:expr, -, $self:ident, $cpsr:ident) => {{
        let res = $a - $b + $c;

        $self.$cpsr.set_c($a >= ($b + $c));
        $self.$cpsr.set_v(((($a ^ $b) & ($a ^ res)) >> 31) != 0);

        res
    }};
}
