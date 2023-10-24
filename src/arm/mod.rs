pub mod interpreter;

/// Fill array with `N` default values besides index `i` which gets `val`.
pub fn arr_with<const N: usize, T: Copy + Default>(i: usize, val: T) -> [T; N] {
    let mut arr = [T::default(); N];
    arr[i] = val;
    arr
}

/// Set V (overflow) and C (carry) flag and save repetition.
///
/// - Set C to carry out of bit31 in ALU.
/// - Set (signed) overflow -- check sign bits of operands and result.
#[macro_export]
macro_rules! fl {
    // ADD, ADC, CMN
    ($a:expr, $b:expr, +, $self:ident, $cpsr:ident $(, $S:expr)?) => {{
        let (res, ov) = $a.overflowing_add($b);
        let set_flags = true $(&& $S)?;

        if set_flags {
            $self.$cpsr.set_c(ov);
            $self
                .$cpsr
                .set_v((($a >> 31) == ($b >> 31)) && (($a >> 31) != (res >> 31)));
        }

        res
    }};

    // SUB, RSB, CMP
    ($a:expr, $b:expr, -, $self:ident, $cpsr:ident $(, $S:expr)?) => {{
        let res = $a - $b;
        let set_flags = true $(&& $S)?;

        if set_flags {
            $self.$cpsr.set_c($a >= $b);
            $self.$cpsr.set_v(((($a ^ $b) & ($a ^ res)) >> 31) != 0);
        }

        res
    }};

    // SBC, RSC
    ($a:expr, $b:expr, $c:expr, -, $self:ident, $cpsr:ident $(, $S:expr)?) => {{
        let res = $a - ($b + $c);
        let set_flags = true $(&& $S)?;

        if set_flags {
            $self.$cpsr.set_c($a >= ($b + $c));
            $self.$cpsr.set_v(((($a ^ $b) & ($a ^ res)) >> 31) != 0);
        }

        res
    }};
}
