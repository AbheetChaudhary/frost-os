use core::fmt;
use core::arch::asm;
use core::cell::SyncUnsafeCell;

// use crate::vga_buffer::SpinLock;
// use crate::mm::HIGHER_BEGIN;

#[inline(always)]
pub unsafe fn outportb(port: u16, val: u8) {
    unsafe {
        asm!(
            "out dx, al",

            in("al") val,
            in("dx") port,
        );
    }
}

#[inline(always)]
pub unsafe fn inportb(port: u16) -> u8 {
    let val: u8;

    unsafe {
        asm!(
            "in al, dx",

            in("dx") port,
            out("al") val,
        )
    }

    val
}

/// Assuming bios environment, this is where the bios puts upto 4 serial port
/// addresses.
// const COM_BASE_ADDR: *const u16 = (HIGHER_BEGIN + 0x0400) as *const _;

/// Line Control Register(LCR) offset from the serial port base.
const LCR_OFFSET: u16 = 3;

/// Offset for the low byte of the divisor. The high byte is after this.
const DIV_LO_OFFSET: u16 = 0;
const DIV_HI_OFFSET: u16 = DIV_LO_OFFSET + 1;

/// FIFO Control Register(FCR) offset.
const FCR_OFFSET: u16 = 2;

/// Interrupt Enable Register(IER) offset.
const IER_OFFSET: u16 = 1;

/// Line Status Register offset.
const LSR_OFFSET: u16 = 5;

/// THRE bit is set when Transmitter Holder Register(THR) is empty. This means
/// that all the character bits have been send to TSR register and now it is
/// safe to write the next character to THR.
const LSR_THRE: u8 = 1 << 5;

/// Tries to initialize the given COM port for serial communication.
///
/// Baud Rate: 38400
/// Mode: 8N1
unsafe fn init(port: u16) {
    unsafe {
        // Disable interrupts.
        outportb(port + IER_OFFSET, 0x00);

        // Disable FIFO, and clear the tx/rx fifo buffers.
        outportb(port + FCR_OFFSET, 0x06);

        // Write to LCR register. DLAB is active now.
        outportb(port + LCR_OFFSET, 0x80);

        // Baud rate of 115200 / 0x0003 = 38400
        let div_lo = 0x03;
        let div_hi = 0x00;

        // Write low and high divisor bytes to set the baud rate.
        outportb(port + DIV_LO_OFFSET, div_lo);
        outportb(port + DIV_HI_OFFSET, div_hi);

        // We are done updating the divisor, clear the DLAB bit.
        outportb(port + LCR_OFFSET, 0x00);

        // Set bits 0 and 1 for using 8-bit data character.
        // Leave bit 2 cleared for 1 bit stop signal.
        // Leave bit 3, 4 and 5 cleared to disable parity.
        // Leave bit 6 cleared.
        let lcr = 0x03;

        outportb(port + LCR_OFFSET, lcr);
    }
}

// pub unsafe fn read(port: u16) -> u8 {
//     unsafe {
//         inportb(port)
//     }
// }

unsafe fn write(port: u16, byte: u8) {
    unsafe {
        // Spin while the Transmit Hold Register still has some bits of
        // earlier written character waiting to be transferred to the TSR.
        while (inportb(port + LSR_OFFSET) & LSR_THRE) == 0 {
            core::hint::spin_loop();
        }

        outportb(port + 0, byte);
    }
}

#[derive(Debug)]
pub struct SerialPort {
    port: Option<u16>,
}

unsafe impl Sync for SerialPort {}
unsafe impl Send for SerialPort {}

// pub static SERIAL: SpinLock<SerialPort> = SpinLock::new(SerialPort { port: None });
pub static SERIAL: SyncUnsafeCell<SerialPort> = SyncUnsafeCell::new(SerialPort { port: None });

impl SerialPort {
    pub unsafe fn init() {
        /*
        let possible_ports = unsafe { core::slice::from_raw_parts(COM_BASE_ADDR, 4) };

        for &port in possible_ports {
            if port == 0 {
                continue;
            }

            unsafe { init(port) };
            SERIAL.lock().port = Some(port);

            break;
        }
        */

        unsafe { init(0x03f8) };
        // SERIAL.lock().port = Some(0x03f8);
        unsafe {
            (&mut *SERIAL.get()).port = Some(0x03f8);
        }
    }

    fn putchar(&mut self, b: u8) {
        if let Some(port) = self.port {
            unsafe { write(port, b) };
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &b in s.as_bytes() {
            self.putchar(b);
        }

        Ok(())
    }
}

#[macro_export]
macro_rules! kserial {
    ($($args:tt)*) => {
        $crate::kserial::_print(format_args!($($args)*));
    };
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    // SERIAL.lock().write_fmt(args).unwrap();
    unsafe {
        // SERIAL.write_fmt(args).unwrap()
        (&mut *SERIAL.get()).write_fmt(args).unwrap()
    }
}

