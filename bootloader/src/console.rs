use crate::{uefi};

struct ConsoleWriter;

impl core::fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        let r = print(s);

        if r.is_success() {
            Ok(())
        } else {
            Err(core::fmt::Error)
        }
    }
}

/// Convert Rust `&str` to utf-16 string on stack and print.
fn print(s: &str) -> uefi::Status {
    // The string is printed in chunks of 31 characters at a time. The last
    // element of this buffer is always a null for null-termination
    // requirements.
    let mut stack_buffer = [0u16; 32];

    let mut idx = 0;
    for c in s.encode_utf16() {
        // If the stack_buffer is filled till the second last element then print
        // everything.
        if idx == stack_buffer.len() - 2 {
            // Second last element might sometimes contain a \n, so zero it
            // out before writing as `raw_efi_print` expects a null terminated
            // string.
            stack_buffer[stack_buffer.len() - 2] = 0;

            // Write the null terminated string.
            let r = unsafe { raw_efi_print(stack_buffer.as_mut_ptr() as *mut uefi::Char16) };
            if !r.is_success() {
                return r;
            }

            // Reset stack_buffer index.
            idx = 0;
        }

        // Replace \n with \r\n. This substitution might write to the second
        // last index of the buffer. If that happens then the buffer just got
        // filled, print it.
        if c == b'\n'.into() {
            // We have already made sure that there is space for at least two
            // more u16's by the above if block.

            stack_buffer[idx] = b'\r'.into();
            idx += 1;

            stack_buffer[idx] = b'\n'.into();
            idx += 1;

            // If the above \r\n substitution fills the buffer up to the last
            // slot then print it. The last slot is the null character.
            if idx == stack_buffer.len() - 1 {
                // Print the null terminated string.
                let r = unsafe { 
                    raw_efi_print(stack_buffer.as_mut_ptr() as *mut uefi::Char16)
                };

                if !r.is_success() {
                    return r;
                }

                // Reset the stack buffer index.
                idx = 0;
            }
        } else {
            // The character is not a newline, just a normal character.
            stack_buffer[idx] = c;
            idx += 1;
        }
    }

    // The buffer might have some residue characters that were filled but did
    // not got to print before the character stream ended. So null terminate the
    // buffer and print them.
    stack_buffer[idx] = 0;

    unsafe { raw_efi_print(stack_buffer.as_mut_ptr() as *mut uefi::Char16) }
}

/// Prints a utf-16 string to screen from the raw pointer to its first character.
///
/// SAFETY: Both pointers must be valid, non-null pointers for their respective
/// types. The string pointer must be null terminating.
///
/// Returned `Status` is a valid `EfiStatus`.
pub unsafe fn raw_efi_print(ptr: *mut uefi::Char16) -> uefi::Status {
    unsafe {
        crate::st_call!(con_out, output_string(ptr))
    }
}

#[doc(hidden)]
pub fn _print_fmt_arguments(args: core::fmt::Arguments) {
    use core::fmt::Write;
    let mut s_w = ConsoleWriter;

    if let Err(_) = s_w.write_fmt(args) {
        panic!("_print_fmt_arguments panicked");
    }
}

/// Macro to write utf-8 strings to console output.
///
/// It works with `\n` terminated strings and should not be used with `\r\n`.
///
/// # Panics
///
/// This macro panics if write to console output fails.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::_print_fmt_arguments(format_args!($($arg)*));
    }
}

/// Macro to write utf-8 strings, followed by a newline to console output.
///
/// Each string is implicitly appended with `\n`. We don't work with CRLF here!
///
/// # Panics
///
/// This macro panics if write to console output fails.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
