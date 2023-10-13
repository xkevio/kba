pub mod interpreter;

/// Set V (overflow) flag and save repetition.
#[macro_export]
macro_rules! ov {
    ($res:expr, $c:ident) => {{
        let (res, ov) = $res;
        $c = ov;
        res
    }};
}
