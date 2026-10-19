#[allow(unused)]
pub use crate::cpu::*;

#[allow(unused)]
pub use crate::serial;

#[allow(unused)]
pub use crate::address::*;

#[macro_export]
macro_rules! as_cast {
    ($e:expr, $src_type:ty => $dst_type:ty) => {{
        let __t: $src_type = From::from($e);
        __t as $dst_type
    }};
}
