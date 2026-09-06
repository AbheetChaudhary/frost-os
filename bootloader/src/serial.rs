use crate::{uefi, proto_call, SERIAL_IO};

/// This is a unit struct, private to this file, with no public constructors.
///
/// It is there so that we could implement `Write` on something. Instances of
/// this type are created whenever writing something to serial output.
struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        let r = serial_write(s);

        // It has to be either success or a warning due to uninitialized global
        // serial IO protocol.
        if r.is_success() || r == uefi::EfiStatus::WarnWriteFailure {
            Ok(())
        } else {
            Err(core::fmt::Error)
        }
    }
}

/// Write a utf8 string to serial output.
fn serial_write(s: &str) -> uefi::Status {
    // Get the pointer to global serial IO protocol, or return with write
    // failure warning.
    let serial = unsafe {
        match SERIAL_IO {
            Some(serial) => serial,
            None => return uefi::EfiStatus::WarnWriteFailure.into(),
        }
    };

    // Byte length of the string.
    let mut byte_count = s.len();

    let r = unsafe {
        proto_call!(serial.write(&mut byte_count, s.as_ptr().cast()))
    };

    if r.is_not_success() {
        panic!("write: {:?}", r);
    } else if byte_count != s.len() {
        panic!("write: tried to write {} bytes, wrote {} bytes", s.len(), 
            byte_count);
    }

    uefi::EfiStatus::Success.into()
}

#[doc(hidden)]
pub fn serial_print_fmt_arguments(args: core::fmt::Arguments) {
    use core::fmt::Write;
    let mut serial_writer = SerialWriter;

    if let Err(_) = serial_writer.write_fmt(args) {
        panic!("serial_print_fmt_arguments failed to write to serial IO.");
    }
}

/// Write a null terminated utf-16 string to serial output.
///
/// Each utf-16 character is first converted to utf-8 before printing. Any
/// invalid characters are converted to [char::REPLACEMENT_CHARACTER] before
/// printing.
///
/// # Safety
///
/// `ptr` must point to a valid, null-terminated utf-16 string.
///
/// # Panics
///
/// This function panics if write to serial output fails. If the serial output
/// has not been initialized yet then this function will silently return before
/// trying to write anything, so no panics in that case.
pub unsafe fn serial_write_raw(ptr: *const uefi::Char16) {
    unsafe {
        // Early return if serial output is not initialized yet.
        if SERIAL_IO == None {
            return;
        }
    }

    let char16_iter = unsafe { crate::Char16Iter::from_char16_ptr(ptr) };

    char::decode_utf16(char16_iter)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .for_each(|c| crate::serial!("{}", c))
}

/// Write a null terminated utf-16 to serial output, followed by newline.
///
/// See [`serial_write_raw`] for more info.
pub unsafe fn serialn_write_raw(ptr: *const uefi::Char16) {
    unsafe {
        serial_write_raw(ptr);
    }
    crate::serial!("\n");
}

/// Macro to write utf-8 strings to serial output.
///
/// It works with `\n` terminated strings and should not be used with `\r\n`.
/// If serial output has not been initialized then it won't panic, nothing will
/// be printed and input arguments will be silently ignored.
///
/// # Panics
///
/// This macro panics if write to serial output fails. If the serial output has
/// not been initialized yet then this macro will silently return before trying
/// to write anything, so no panics in that case.
#[macro_export]
macro_rules! serial {
    ($($arg:tt)*) => { $crate::serial::serial_print_fmt_arguments(format_args!($($arg)*)) }
}

/// Macro to write utf-8 strings followed by newline to serial output.
///
/// It works with `\n` terminated strings and should not be used with `\r\n`.
/// If serial output has not been initialized then it won't panic, nothing will
/// be printed and input arguments will be silently ignored.
///
/// # Panics
///
/// This macro panics if write to serial output fails. If the serial output has
/// not been initialized yet then this macro will silently return before trying
/// to write anything, so no panics in that case.
#[macro_export]
macro_rules! serialn {
    () => { $crate::serial!("\n") };
    ($($arg:tt)*) => { $crate::serial!("{}\n", format_args!($($arg)*)) }
}
