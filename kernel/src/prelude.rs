pub use crate::cpu::*;
pub use crate::serial;
pub use crate::address::*;

#[macro_export]
macro_rules! as_cast {
    ($e:expr, $src_type:ty => $dst_type:ty) => {{
        let __t: $src_type = From::from($e);
        __t as $dst_type
    }};
}
