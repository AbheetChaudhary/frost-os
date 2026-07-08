use crate::{irq, isr, println};

use irq::{outb, io_wait};

fn timer_phase(hz: usize) {
    let div: usize = 1193180 / hz;
    outb(0x43, 0x36);
    io_wait();

    outb(0x40, (div & 0xFF) as u8);
    io_wait();

    outb(0x40, (div >> 8) as u8);
    io_wait();
}

pub fn install() {
    irq::IRQRoutines::install_handler(0, timer_handler);
    timer_phase(100);
}

static mut TIMER_TICKS: usize = 0;

fn timer_handler(regs: &isr::RegState) {
    let ticks;
    unsafe { 
        TIMER_TICKS += 1;
        ticks = TIMER_TICKS;
    }

    if ticks != 0 && ticks % 100 == 0 {
        println!("one second");
    }
}
