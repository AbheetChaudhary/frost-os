use core::sync::atomic::{AtomicBool, Ordering};
use core::ptr::NonNull;
use core::fmt;
use core::cell::UnsafeCell;

pub static WRITER: SpinLock<VgaBuffer> = {
    // SAFETY:
    // It is the only instance of VgaBuffer.
    let vga_buffer: VgaBuffer = unsafe { VgaBuffer::new() };
    SpinLock::new(vga_buffer)
};


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
                        lock: self,
                        marker: core::marker::PhantomData,
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
    lock: &'lock SpinLock<T>,
    marker: core::marker::PhantomData<*const ()>, // no sync send, covariant
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

pub struct CurrentColor(u8);

impl CurrentColor {
    /// Generate a CurrentColor from background and foreground Color and blink.
    /// Only the lower 3 bits of the background are used, the upper most bit is
    /// taken from blink.
    pub const fn from(bg: Color, fg: Color, blink: bool) -> Self {
        let bg: u8 = bg as u8;
        let fg: u8 = fg as u8;
        let blink: u8 = blink as u8;

        let current_color = (blink << 7) | (bg << 4) | fg;

        CurrentColor(current_color)
    }
}

pub struct VgaBuffer {
    ptr:   NonNull<u16>,
    color: CurrentColor,
    pos:   usize, // position for the next character to go
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
            ptr:   NonNull::new(0xb8000 as *mut u16).unwrap(),
            color: CurrentColor::from(Color::Black, Color::LightGreen, false),
            pos:   0,
        }
    }

    /// Write a byte to the vga buffer. If that byte is newline then add a
    /// newline and go to the beginning of the new line.
    ///
    /// VGA follows 'code page 437' encoding. Every byte corresponds to a
    /// printable character, even control characters as well. They just print
    /// arbitrary emoji and hearts.
    pub fn write_byte(&mut self, byte: u8) {
        let character: u16 = u16::from_le_bytes([byte, self.color.0]);

        match byte {
            b'\n' => {
                self.new_line();
            }

            _ => {
                unsafe {
                    self.ptr.add(self.pos).write_volatile(character);
                }
                self.pos += 1;
            }
        }
    }

    pub fn write_string(&mut self, text: &str) {
        for byte in text.bytes() {
            match byte {
                // Printable ASCII characters.
                0x20..=0x7e | b'\n' => self.write_byte(byte),

                // For every non printable character print square.
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn clear_screen(&mut self) {
        for i in 0..25 {
            self.scroll_up();
        }
    }

    pub fn change_color(&mut self, to: CurrentColor) {
        self.color = to;
    }

    pub fn new_line(&mut self) {
        let row = self.pos / WIDTH;
        // let col = self.pos % WIDTH;

        if row < HEIGHT - 1 {
            // Cursor is not at the last line, it could go one line lower.
            //
            // Set it to beginning of next line.
            self.pos = (row + 1) * WIDTH;
        } else {
            self.scroll_up();

            // Bring the cursor to the beginning of the line.
            self.pos = row * WIDTH;
        }
    }

    /// This moves everything one line up, creating a new blank line at the
    /// bottom. The cursor position remains same with respect to the text except
    /// in the case when the cursor was on the first line, in that case the
    /// `self.pos` value does not changes.
    fn scroll_up(&mut self) {
        // Move everything from second line till last line one line up.
        // Since there is no volatile copy, we must copy each character
        // individually to its new location, that is on the line above.
        for idx in WIDTH..BUFFER_SIZE {
            unsafe {
                let c = self.ptr.add(idx).read_volatile();
                self.ptr.add(idx - WIDTH).write_volatile(c);
            }
        }

        // Clear the last line, with volatile writes.
        for i in 0..WIDTH {
            unsafe {
                self.ptr.add(WIDTH * (HEIGHT - 1) + i).write_volatile(0);
            }
        }

        // Set new cursor position to just above current position.
        let row = self.pos / WIDTH;
        self.pos = if row > 0 {
            // Just above the current position.
            self.pos - WIDTH
        } else {
            // Already at first line, no change.
            self.pos
        };
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
