use core::arch::asm;

use crate::{isr, println};

unsafe extern "C" {
    fn _irq0();  fn _irq1();  fn _irq2();  fn _irq3();
    fn _irq4();  fn _irq5();  fn _irq6();  fn _irq7();
    fn _irq8();  fn _irq9();  fn _irq10(); fn _irq11();
    fn _irq12(); fn _irq13(); fn _irq14(); fn _irq15();
}

#[inline(never)]
fn no_irq(_: &isr::RegState) {
    panic!("No IRQ set!");
}

pub static mut GLOBAL_IRQ_ROUTINES: [usize; 16] = [0; 16];

pub struct IRQRoutines;

impl IRQRoutines {
    pub fn install_handler(irq_idx: usize, handler: fn(&isr::RegState)) {
        if irq_idx > 15 {
            panic!("irq_idx: {} is greater than 15", irq_idx);
        }

        unsafe {
            let ptr = (&raw mut GLOBAL_IRQ_ROUTINES).cast::<usize>();
            let handler = core::mem::transmute(handler);
            ptr.add(irq_idx).write_volatile(handler);
        }
        println!("installed something");
    }

    pub fn uninstall_handler(irq_idx: usize) {
        if irq_idx > 15 {
            panic!("irq_idx: {} is greater than 15", irq_idx);
        }

        unsafe {
            let ptr = (&raw mut GLOBAL_IRQ_ROUTINES).cast::<usize>();
            ptr.add(irq_idx).write_volatile(0);
        }
    }

    fn get_handler(irq_idx: usize) -> Option<fn(&isr::RegState)> {
        if irq_idx > 15 {
            panic!("irq_idx: {} is greater than 15", irq_idx);
        }

        unsafe {
            let ptr = (&raw const GLOBAL_IRQ_ROUTINES).cast::<usize>();

            let handler = ptr.add(irq_idx).read_volatile();

            if handler != 0 {
                Some(core::mem::transmute(handler))
            } else {
                None
            }
        }
    }
}

const PIC1:         u16 = 0x20; /* IO base address for master PIC */
const PIC2:         u16 = 0xA0; /* IO base address for slave PIC */
const PIC1_COMMAND: u16 = PIC1;
const PIC1_DATA:    u16 = (PIC1+1);
const PIC2_COMMAND: u16 = PIC2;
const PIC2_DATA:    u16 = (PIC2+1);
const PIC_EIO:       u8 = 0x20;

const ICW1_ICW4:      u8 = 0x01;
const ICW1_SINGLE:    u8 = 0x02;
const ICW1_INTERVAL4: u8 = 0x04;
const ICW1_LEVEL:     u8 = 0x08;
const ICW1_INIT:      u8 = 0x10;

const ICW4_8086:       u8 = 0x01;
const ICW4_AUTO:       u8 = 0x02;
const ICW4_BUF_SLAVE:  u8 = 0x08;
const ICW4_BUF_MASTER: u8 = 0x0C;
const ICW4_SFNM:       u8 = 0x10;

const CASCADE_IRQ: u8 = 2;

#[inline(always)]
pub fn outb(port: u16, val: u8) {
    unsafe {
        asm!(
            "out dx, al",

            in("al") val,
            in("dx") port,
        );
    }
}

#[inline(always)]
pub fn io_wait() { // Needed on older machines to give PIC some time to react.
    outb(0x80, 0);
}

fn pic_remap() {
    const OFFSET_1: u8 = 32; // vector offset for master PIC
    const OFFSET_2: u8 = 40; // vector offset for slave PIC

    outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
    io_wait();

    outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
    io_wait();

    outb(PIC1_DATA, OFFSET_1);
    io_wait();

    outb(PIC2_DATA, OFFSET_2);
    io_wait();

    outb(PIC1_DATA, 1 << CASCADE_IRQ);
    io_wait();

    outb(PIC2_DATA, 2);
    io_wait();

    outb(PIC1_DATA, ICW4_8086);
    io_wait();

    outb(PIC2_DATA, ICW4_8086);
    io_wait();

    outb(PIC1_DATA, 0);
    outb(PIC2_DATA, 0);
}

fn pic_send_eio(irq: u8) {
    if irq >= 8 {
        outb(PIC2_COMMAND, PIC_EIO);
    }
    outb(PIC1_COMMAND, PIC_EIO);
}

pub fn irq_install() {
    pic_remap();

    isr::IDT::set_gate(32, _irq0,  0x08, 0x8E);
    isr::IDT::set_gate(33, _irq1,  0x08, 0x8E);
    isr::IDT::set_gate(34, _irq2,  0x08, 0x8E);
    isr::IDT::set_gate(35, _irq3,  0x08, 0x8E);
    isr::IDT::set_gate(36, _irq4,  0x08, 0x8E);
    isr::IDT::set_gate(37, _irq5,  0x08, 0x8E);
    isr::IDT::set_gate(38, _irq6,  0x08, 0x8E);
    isr::IDT::set_gate(39, _irq7,  0x08, 0x8E);
    isr::IDT::set_gate(40, _irq8,  0x08, 0x8E);
    isr::IDT::set_gate(41, _irq9,  0x08, 0x8E);
    isr::IDT::set_gate(42, _irq10, 0x08, 0x8E);
    isr::IDT::set_gate(43, _irq11, 0x08, 0x8E);
    isr::IDT::set_gate(44, _irq12, 0x08, 0x8E);
    isr::IDT::set_gate(45, _irq13, 0x08, 0x8E);
    isr::IDT::set_gate(46, _irq14, 0x08, 0x8E);
    isr::IDT::set_gate(47, _irq15, 0x08, 0x8E);
}

#[unsafe(no_mangle)]
pub extern "C" fn irq_handler(regs: &isr::RegState) {
    let irq = regs.int_no as u8 - 32;
    let irq = irq as usize;

    // println!("RegState: {:#?}", regs);

    // println!("going to handle IRQ#{}, error code: {}", irq, regs.err_code as u8);

    if let Some(handler) = IRQRoutines::get_handler(irq) {
        handler(regs);
    }

    // println!("handled");

    // Send EOI command.
    pic_send_eio(irq as u8);
}

