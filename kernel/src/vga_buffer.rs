use core::sync::atomic::{AtomicBool, Ordering};
use core::ptr::NonNull;
use core::fmt;
use core::cell::UnsafeCell;

// use crate::mm::HIGHER_BEGIN;

pub static WRITER: SpinLock<VgaBuffer> = {
    // SAFETY: It is the only instance of VgaBuffer.
    let vga_buffer: VgaBuffer = unsafe { VgaBuffer::new() };
    SpinLock::new(vga_buffer)
};


// DROP?
pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

/// SpinLock has to be Sync because other threads will need a reference to it to
/// acquire the lock.
///
/// Having a reference to SpinLock allows one to call `lock()` on it, getting a
/// mutable reference to inner type `T`. Thus `T` must be `Send`. `Sync` is not
/// necessary because multiple threads are not having concurrent access to the
/// inner `T`, at a time only one thread could have an exclusive reference to
/// the inner `T`.
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// SpinLock is `Send` if `T` is `Send`. If another thread gets ownership of
/// spinlock then it could get an exclusive reference to inner `T`, thus
/// requiring the bound of `Send` for `T`.
unsafe impl<T: Send> Send for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinGuard<'_, T> {
        loop {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }

            let exchange = self.locked.compare_exchange(false, 
                true, Ordering::Acquire, Ordering::Relaxed);

            match exchange {
                Ok(_) => {
                    break SpinGuard {
                        lock:     self,
                        __marker: core::marker::PhantomData,
                    };
                }
                Err(_) => {
                    continue;
                }
            }
        }
    }
}

pub struct SpinGuard<'lock, T> {
    lock:     &'lock SpinLock<T>,
    __marker: core::marker::PhantomData<*const ()>, // no sync send, covariant
}

impl<'lock, T> Drop for SpinGuard<'lock, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

impl<'lock, T> core::ops::Deref for SpinGuard<'lock, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'lock, T> core::ops::DerefMut for SpinGuard<'lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Color {
    Black = 0x0,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    LightGray,
    DarkGray,
    LightBlue,
    LightGreen,
    LightCyan,
    LightRed,
    Pink,
    Yellow,
    White = 0xf,
}

impl From<Color> for u8 {
    fn from(value: Color) -> u8 {
        value as u8
    }
}

const WIDTH:       usize = 80;
const HEIGHT:      usize = 25;
const BUFFER_SIZE: usize = WIDTH * HEIGHT;

#[derive(Copy, Clone)]
pub struct Attribute(u8);

impl Attribute {
    /// Generate an Attribute from background and foreground Color and blink.
    ///
    /// Only the lower 3 bits of the background are used, the upper most bit is
    /// taken from blink.
    pub const fn from(bg: Color, fg: Color, blink: bool) -> Self {
        let bg: u8 = bg as u8;
        let fg: u8 = fg as u8;
        let blink: u8 = blink as u8;

        let attr = (blink << 7) | (bg << 4) | fg;

        Attribute(attr)
    }

    pub fn with_blink(&self, blink: bool) -> Self {
        let b = (blink as u8) << 7;
        let a = self.0 & 0x7f;

        Attribute(a | b)
    }
}

#[derive(Default)]
struct Position {
    x: usize, 
    y: usize,
}

impl Position {
    fn as_idx(&self) -> usize {
        self.y * WIDTH + self.x
    }
}

pub struct VgaBuffer {
    ptr:  NonNull<u16>,
    attr: Attribute,
    pos:  Position,           // position for the next character to go
}

/// It needs `Send` to satisfy the bounds for `SpinLock`, but it will never be
/// send across thread boundary. There is only one vga buffer and that is behind
/// a global static spin lock. Sending it across would require to create a new
/// instance of vga buffer to fill inside the spin lock, and creating new
/// instances is prohibited by marking the only way to create a vga buffer as
/// unsafe.
unsafe impl Send for VgaBuffer {}

impl VgaBuffer {
    /// VgaBuffer is always used through its single global instance that is
    /// behind a spin lock. Due to being in spin lock, other threads could get a
    /// mutable reference to the one true vga buffer. They could thus move it
    /// out of the global spin lock. But anyone trying to move it must replace
    /// the original one with a new instance, and if the only way to create a
    /// new instance requires unsafe then no one will try doing so.
    ///
    /// Never implement `Default` or `Copy` for this.
    ///
    /// SAFETY:
    /// It is marked unsafe to make sure that nobody initializes another
    /// instance of it. Just use the global one.
    pub const unsafe fn new() -> Self {
        VgaBuffer {
            ptr:  NonNull::new((/* HIGHER_BEGIN + */ 0xb8000) as *mut u16).unwrap(),
            attr: Attribute::from(Color::Black, Color::LightGreen, false),
            pos:  Position { x: 0, y: 0 },
        }
    }

    /// Write a byte to the vga buffer. If that byte is newline then add a
    /// newline and go to the beginning of the new line.
    ///
    /// VGA follows 'code page 437' encoding. Every byte corresponds to a
    /// printable character, even control characters as well. They just print
    /// arbitrary emoji and hearts.
    pub fn write_byte_and_advance_cursor(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.enter_new_line();
            }

            // backspace
            b'\x08' => {
                // Move the cursor one step back, and write space to the new
                // location. Do nothing if cursor is already at the beginning.

                // TODO: what should be the correct attribute to write the
                // replacing space with?
                //
                // Currently we are using the same attribute with which the last
                // character was written. Looks reasonable for now.
                //
                // TODO: Backspace at the beginning of a line unconditionally
                // puts us to the end of the previous line(if not already at the
                // first line), regardless of whether there was any character
                // written or not. So backspaces after writing a newline are not
                // functioning correctly.
                if self.pos.x > 0 {
                    self.pos.x -= 1;
                    unsafe { self.putchar(b' ', self.attr.with_blink(false)) };
                } else if self.pos.y > 0 {
                    self.pos.y -= 1;
                    self.pos.x = WIDTH - 1;
                    unsafe { self.putchar(b' ', self.attr.with_blink(false)) };
                }
            }

            _ => {
                unsafe { self.putchar(byte, self.attr); }

                self.advance_cursor();
            }
        }
    }

    /// Utility function to just write byte at the current cursor location,
    /// using the given attribute without advancing the cursor.
    ///
    /// SAFETY: The current cursor position must not be out of bounds of the VGA
    /// text buffer.
    unsafe fn putchar(&mut self, byte: u8, attr: Attribute) {
        let character = u16::from_le_bytes([byte, attr.0]);

        let idx = self.pos.as_idx();

        unsafe {
            self.ptr.add(idx).write_volatile(character);
        }
    }

    fn advance_cursor(&mut self) {
        // If we are at the last cell of current line then enter newline,
        // otherwise just move the cursor one step right.
        if self.pos.x == WIDTH - 1 {
            self.enter_new_line();
        } else {
            self.pos.x += 1;
        }
    }

    /// SAFETY: The new position must valid cell position inside the 80x25 VGA
    /// buffer.
    unsafe fn set_position(&mut self, pos: (usize, usize)) {
        self.pos.x = pos.0;
        self.pos.y = pos.1;
    }

    pub fn write_string(&mut self, text: &str) {
        for byte in text.bytes() {
            match byte {
                // Printable ASCII characters.
                0x20..=0x7e | b'\n' | b'\x08' => self.write_byte_and_advance_cursor(byte),

                // For every non printable character print square.
                _ => self.write_byte_and_advance_cursor(0xfe),
            }
        }
    }

    /// Clear the screen by writing space to all the cells and bring the cursor
    /// to the beginning.
    pub fn clear_screen(&mut self) {
        for i in 0..BUFFER_SIZE {
            unsafe { self.ptr.add(i).write_volatile(0); }
        }

        self.pos = Position::default();
    }

    /// Sets the cursor position to the beginning of the next line. If already
    /// at the last line then scrolls the view up, and sets the cursor at the
    /// beginning of the new emptied last line.
    pub fn enter_new_line(&mut self) {
        if self.pos.y < HEIGHT - 1 {
            // Cursor is not at the last line, it could go one line lower.
            //
            // Set it to beginning of next line.
            self.pos = Position { x: 0, y: self.pos.y + 1 };
        } else {
            // Cursor is at the last line.
            self.scroll_up();

            // Bring the cursor to the beginning of the last line.
            self.pos = Position { x: 0, y: HEIGHT - 1 };
        }
    }

    /// This moves everything one line up, creating a new blank line at the
    /// bottom.
    fn scroll_up(&mut self) {
        // Move everything from second line till last line one line up.
        // Since there is no volatile copy(), we must copy each character
        // individually to its new location, that is on the line above.
        for idx in WIDTH..BUFFER_SIZE {
            unsafe {
                let c = self.ptr.add(idx).read_volatile();
                self.ptr.add(idx - WIDTH).write_volatile(c);
            }
        }

        // Clear the last line, with volatile writes.
        let begin = unsafe { self.ptr.add(WIDTH * HEIGHT - WIDTH) };
        for i in 0..WIDTH {
            // Write space in black with no blink in each cell.
            unsafe {
                begin.add(i).write_volatile(0);
            }
        }
    }
}

impl fmt::Write for VgaBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
}
