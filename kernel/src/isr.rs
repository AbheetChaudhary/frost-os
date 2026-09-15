use core::arch::asm;
use core::mem::MaybeUninit;

// use crate::{serial};
use crate::prelude::*;

// These are function pointers to ISR's from 0-31. All of them are reserved for
// CPU exceptions.
#[link(name = "isr.o", kind = "static", modifiers = "+verbatim")]
unsafe extern "C" {
    fn _isr0();  fn _isr1();  fn _isr2();  fn _isr3();  
    fn _isr4();  fn _isr5();  fn _isr6();  fn _isr7();  
    fn _isr8();  fn _isr9();  fn _isr10(); fn _isr11();
    fn _isr12(); fn _isr13(); fn _isr14(); fn _isr15(); 
    fn _isr16(); fn _isr17(); fn _isr18(); fn _isr19();
    fn _isr20(); fn _isr21(); fn _isr22(); fn _isr23();
    fn _isr24(); fn _isr25(); fn _isr26(); fn _isr27();
    fn _isr28(); fn _isr29(); fn _isr30(); fn _isr31();
}

/// Funciton pointers to ISR's reserved for CPU exceptions.
const EXCEPTION_ISR_FN_PTRS: [unsafe extern "C" fn(); 32] = [
    _isr0,  _isr1,  _isr2,  _isr3,  _isr4,  _isr5,  _isr6,  _isr7,  
    _isr8,  _isr9,  _isr10, _isr11, _isr12, _isr13, _isr14, _isr15, 
    _isr16, _isr17, _isr18, _isr19, _isr20, _isr21, _isr22, _isr23,
    _isr24, _isr25, _isr26, _isr27, _isr28, _isr29, _isr30, _isr31,
];

#[repr(C, packed)]
pub struct Idtr {
    size: u16,
    base: u32,
}

static mut IDTR: Idtr = Idtr { size: 0, base: 0 };

static mut GLOBAL_IDT: [IDTEntry; 256] = [IDTEntry::const_default(); 256];

pub fn get_idtr_address() -> *const Idtr {
    &raw const IDTR
}

/// The `isr_common_stub` pushes the registers on stack in a fixed order that
/// can be referenced as this struct.
#[derive(Debug)]
#[repr(C)]
pub struct RegState {
    gs:       u32,
    fs:       u32,
    es:       u32,
    ds:       u32,
    edi:      u32,
    esi:      u32,
    ebp:      u32,
    esp:      u32,
    ebx:      u32,
    edx:      u32,
    ecx:      u32,
    eax:      u32,

    /// Interrupt number as pushed by the ISR before jumping to the
    /// `isr_common_stub`
    pub int_no:   u32,

    /// Error code that is part of the interrupt. Not all interrupts have an
    /// error code, so the ISR pushes a dummy error code just to keep a fixed
    /// structure for the stack.
    pub err_code: MaybeUninit<u32>,

    ip:       u32,
    cs:       u32,
    flags:    u32,

    // These two may not always be there. They are present only if interrupt
    // occured when program was in another privilege level when the interrupt
    // occured.
    user_sp:  MaybeUninit<u32>,
    ss:       MaybeUninit<u32>,
}

/// Interrupt handlers will get a pointer to the RegState as pushed by the
/// `isr_common_stub` in the assembly code.
///
/// The interrupt/exception handler functions should not except a reference to
/// RegState, but a raw pointer. The pointer to RegState as created on the stack
/// may not be properly aligned so `read_unaligned()` must be used to read its
/// fields.
pub type InterruptStackFrame = *const RegState;

/// Error message for 32 reserved exceptions. Some are meaningful, others are
/// just "Reserved Exception".
const EXCEPTION_MESSAGES: [&str; 32] = [
    "Division By Zero Exception",
    "Debug Exception",
    "Non Maskable Interrupt Exception",
    "Breakpoint Exception",
    "Into Detected Overflow Exception",
    "Out of Bounds Exception",
    "Invalid Opcode Exception",
    "No Co-processor Exception",
    "Double Fault Exception",
    "Co-processor Segment Overrun Exception",
    "Bad TSS Exception",
    "Segment Not Present Exception",
    "Stack Fault Exception",
    "General Protection Fault Exception",
    "Page Fault Exception",
    "Unknown Interrupt Exception",
    "Coprocessor Fault Exception",
    "Alignment Check Exception (486+)",
    "Machine Check Exception (Pentium/586+)",
    "Reserved Exception (19)",
    "Reserved Exception (20)",
    "Reserved Exception (21)",
    "Reserved Exception (22)",
    "Reserved Exception (23)",
    "Reserved Exception (24)",
    "Reserved Exception (25)",
    "Reserved Exception (26)",
    "Reserved Exception (27)",
    "Reserved Exception (28)",
    "Reserved Exception (29)",
    "Reserved Exception (30)",
    "Reserved Exception (31)",
];

#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct IDTEntry {
    pub offset_lo:  u16,
    pub seg_sel:    u16,
    pub zero:       u8,
    pub type_attrs: u8,
    pub offset_hi:  u16,
}

impl IDTEntry {
    const fn const_default() -> Self {
        Self {
            offset_lo:  0,
            seg_sel:    0,
            zero:       0,
            type_attrs: 0,
            offset_hi:  0,
        }
    }
}

/// For now, all the exceptions have an ISR that eventually reaches here.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn exception_handler(regs: InterruptStackFrame) {
    let int_no = unsafe { (&raw const (*regs).int_no).read_unaligned() };

    if int_no < 32 {
        serial!("{}", EXCEPTION_MESSAGES[int_no as usize]);
        serial!(". System Halted!\n");

        loop {}
    }

    unreachable!("Reached exception_handler() but not exception occured.");
}

pub struct IDT;

impl IDT {
    pub fn set_gate(idx: usize, isr: unsafe extern "C" fn(), seg_sel: u16, type_attrs: u8) {
        let func_addr = isr as u32;

        let idt_entry = IDTEntry {
            offset_lo:  (func_addr & 0xFFFF) as u16,
            seg_sel:    seg_sel,
            zero:       0,
            type_attrs: type_attrs,
            offset_hi:  ((func_addr >> 16) & 0xFFFF) as u16,
        };

        unsafe { GLOBAL_IDT[idx] = idt_entry; }
    }

    fn flush() {
        unsafe {
            IDTR.size = 256 * size_of::<IDTEntry>() as u16 - 1;
            IDTR.base = &raw const GLOBAL_IDT as u32;
        }

        unsafe {
            asm!(
                "lidt [{}]",

                in(reg) (&raw const IDTR),
            );
        }
    }
}

pub unsafe fn isr_install() {
    // Index of the code segment in the GDT.
    let code_segment_idx = 0x08;

    // Interrupt gate type attribute.
    let type_attrs = 0x8e;

    for i in 0..32 {
        IDT::set_gate(i, EXCEPTION_ISR_FN_PTRS[i], code_segment_idx, type_attrs);
    }

    IDT::flush();
}
