use crate::{irq, isr};

use irq::{outportb, io_wait, DEFAULT_WAIT_UNUSED_PORT};

// PIT 8253/8254
//
// Default timer frequency: 18.222 Hz

/// Number of ticks since the timer was installed.
///
/// At 100 ticks per second rate, it will take over 460 days to overflow.
static mut SYSTEM_TIMER_TICKS: usize = 0;

/// IRQ number for the PIT chips channel-0. It is not an index in the IDT.
const TIMER_IRQ_NUMBER: u8 = 0;

/// Frequency at which PIT Channel-0 fires interrupts.
pub const SYSTEM_TIMER_FREQ: usize = 100;

unsafe fn timer_phase(hz: usize) {
    let div: usize = 1193180 / hz;

    unsafe {
        outportb(0x43, 0x36);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        outportb(0x40, (div & 0xFF) as u8);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        outportb(0x40, (div >> 8) as u8);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);
    }
}

pub fn install() {
    unsafe {
        // Default to 100 timer interrupts per second.
        timer_phase(SYSTEM_TIMER_FREQ);

        irq::IRQRoutines::install_handler(TIMER_IRQ_NUMBER, timer_irq_handler);
    }
}

unsafe extern "C" fn timer_irq_handler(_regs: isr::InterruptStackFrame) {
    unsafe { SYSTEM_TIMER_TICKS += 1 };
}

/// Gets the number of ticks passed since the system timer was started.
///
/// At 100 ticks/second rate, each tick means 10 millisecond.
pub fn get_current_ticks() -> usize {
    unsafe { SYSTEM_TIMER_TICKS }
}
