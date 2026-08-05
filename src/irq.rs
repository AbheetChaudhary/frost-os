use core::arch::asm;

use crate::{isr};

#[link(name = "irq.o", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn _irq0();  fn _irq1();  fn _irq2();  fn _irq3();
    fn _irq4();  fn _irq5();  fn _irq6();  fn _irq7();
    fn _irq8();  fn _irq9();  fn _irq10(); fn _irq11();
    fn _irq12(); fn _irq13(); fn _irq14(); fn _irq15();
}

type IrqHandlerFn = unsafe extern "C" fn(isr::InterruptStackFrame);

/// All the IRQ handler function pointers are stored here. The `irq_handler`
/// function looks for the IRQ number and calls the corresponding function if
/// the function pointer is not null.
///
/// Although it is possible that the 0 address is a valid function pointer for
/// an handler if someone sets up things that way. But it is very unlikely
/// otherwise.
static mut GLOBAL_IRQ_HANDLER_ROUTINES: [usize; 16] = [0; 16];

pub struct IRQRoutines;

impl IRQRoutines {
    pub unsafe fn install_handler(irq_idx: u8, handler: IrqHandlerFn) {
        if irq_idx > 15 {
            panic!("irq_idx: {} is greater than 15", irq_idx);
        }

        unsafe {
            let ptr = (&raw mut GLOBAL_IRQ_HANDLER_ROUTINES).cast::<usize>();
            let handler = core::mem::transmute(handler);
            ptr.add(irq_idx as usize).write_volatile(handler);
        }

        // println!("installed handler for IRQ{}", irq_idx);
    }

    pub unsafe fn uninstall_handler(irq_idx: u8) {
        if irq_idx > 15 {
            panic!("irq_idx: {} is greater than 15", irq_idx);
        }

        unsafe {
            let ptr = (&raw mut GLOBAL_IRQ_HANDLER_ROUTINES).cast::<usize>();
            ptr.add(irq_idx as usize).write_volatile(0);
        }
    }

    fn get_handler(irq_idx: u8) -> Option<IrqHandlerFn> {
        if irq_idx > 15 {
            panic!("irq_idx: {} is greater than 15", irq_idx);
        }

        unsafe {
            let ptr = (&raw const GLOBAL_IRQ_HANDLER_ROUTINES).cast::<usize>();

            let handler = ptr.add(irq_idx as usize).read_volatile();

            if handler != 0 {
                Some(core::mem::transmute(handler))
            } else {
                None
            }
        }
    }
}

// IO base address for master PIC
const PIC1:         u16 = 0x20;

// IO base address for slave PIC
const PIC2:         u16 = 0xA0;

/// Command and data port address for master PIC.
const PIC1_COMMAND: u16 = PIC1;
const PIC1_DATA:    u16 = PIC1+1;

/// Command and data port address for slave PIC.
const PIC2_COMMAND: u16 = PIC2;
const PIC2_DATA:    u16 = PIC2+1;

/// End Of Interrupt
const PIC_EIO:       u8 = 0x20;

const ICW1_ICW4:       u8 = 0x01;
const _ICW1_SINGLE:    u8 = 0x02;
const _ICW1_INTERVAL4: u8 = 0x04;
const _ICW1_LEVEL:     u8 = 0x08;
const ICW1_INIT:       u8 = 0x10;

const ICW4_8086:        u8 = 0x01;
const _ICW4_AUTO:       u8 = 0x02;
const _ICW4_BUF_SLAVE:  u8 = 0x08;
const _ICW4_BUF_MASTER: u8 = 0x0C;
const _ICW4_SFNM:       u8 = 0x10;

/// Slave PIC is wired to IRQ2 of master.
const CASCADE_IRQ: u8 = 2;

/// Unused port that could be written to in order to add a predictable amount of
/// delay in PIC I/O operations. This is the one that linux uses.
pub const DEFAULT_WAIT_UNUSED_PORT: u16 = 0x80;

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

/// Needed on older machines to give PIC some time to react. Uses an unused port
/// and writes to it.
#[inline(always)]
pub unsafe fn io_wait(unused_port: u16) {
    unsafe { outportb(unused_port, 0); }
}

/* Taken from OSdev wiki, 8259 PIC Article */

const OFFSET_1: u8 = 32;           // vector offset for master PIC
const OFFSET_2: u8 = OFFSET_1 + 8; // vector offset for slave PIC

/// Remap IRQ 0-15 to IDT entry 32-47. This is requried because by default the
/// IRQ's are mapped to some exception reserved ISR's in the interrupt descriptor
/// table.
fn pic_remap() {
    unsafe { 
        // Start initialization in cascade mode. The PIC's will wait for the offset, 
        // master-slave cascading information, and additional information about the
        // environment.
        outportb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);
        outportb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        // Master PIC vector offset.
        outportb(PIC1_DATA, OFFSET_1);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        // Slave PIC vector offset.
        outportb(PIC2_DATA, OFFSET_2);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        // Tell master that there is a slave PIC at IRQ2.
        outportb(PIC1_DATA, 1 << CASCADE_IRQ);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        // Tell slave PIC its cascade identity.
        outportb(PIC2_DATA, CASCADE_IRQ);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        // Make PIC's use 8086 mode.
        outportb(PIC1_DATA, ICW4_8086);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);
        outportb(PIC2_DATA, ICW4_8086);
        io_wait(DEFAULT_WAIT_UNUSED_PORT);

        // Unmask both PICs.
        outportb(PIC1_DATA, 0);
        outportb(PIC2_DATA, 0);
    }
}

/// Send end of interrupt(EOI) command to PIC after interrupt has been
/// successfully handled.
unsafe fn pic_send_eio(irq: u8) {
    unsafe {
        // IRQ came from slave PIC, so it must be send EOI as well.
        if irq >= 8 {
            outportb(PIC2_COMMAND, PIC_EIO);
        }

        outportb(PIC1_COMMAND, PIC_EIO);
    }
}

pub unsafe fn irq_install() {
    // Remap the IRQs by reprogramming the PIC.
    pic_remap();

    // Install IRQs starting from OFFSET_1 = 32.
    assert!(OFFSET_1 == 32 && OFFSET_2 == 40, "Tried to initialize IRQ at wrong offsets");

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
#[inline(never)]
pub unsafe extern "C" fn irq_handler(regs: isr::InterruptStackFrame) {
    let int_no = unsafe { (&raw const (*regs).int_no).read_unaligned() };

    let irq = int_no as u8 - OFFSET_1;

    // println!("going to handle IRQ#{}", irq);

    if let Some(handler) = IRQRoutines::get_handler(irq) {
        unsafe { handler(regs); }
    } else {
        panic!("Tried to handle IRQ#{}, but it had no handler installed.", irq);
    }

    // println!("handled IRQ#{}", irq);

    // Send EOI command.
    unsafe { pic_send_eio(irq as u8); }
}

