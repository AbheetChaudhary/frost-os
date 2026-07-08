use core::arch::asm;

use crate::{println, print};

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

#[repr(C, packed)]
struct IDTR {
    size: u16,
    base: u32,
}

unsafe extern "C" {
    static mut _idtr: IDTR;
}

static mut GLOBAL_IDT: [IDTEntry; 256] = [IDTEntry::const_default(); 256];

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
    pub int_no:   u32,
    pub err_code: u32,
    ip:       u32,
    cs:       u32,
    flats:    u32,
    sp:       u32,
    ss:       u32,
}

const EXCEPTION_MESSAGES: [&str; 32] = [
    "Division By Zero Exception",
    "Debug Exception",
    "Non Maskable Interrupt Exception",
    "Breakpoint Exception",
    "Into Detected Overflow Exception",
    "Out of Bounds Exception",
    "Invalid Opcode Exception",
    "No Coprocessor Exception",
    "Double Fault Exception",
    "Coprocessor Segment Overrun Exception",
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

#[unsafe(no_mangle)]
pub extern "C" fn fault_handler(regs: &RegState) {
    if regs.int_no < 32 {
        print!("{}", EXCEPTION_MESSAGES[regs.int_no as usize]);
        println!(". System Halted!");

        loop {}
    }
}

pub struct IDT;

impl IDT {
    pub fn set_gate(
        idx: usize,
        isr: unsafe extern "C" fn(),
        seg_sel: u16,
        type_attrs: u8
    ) {
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

    pub fn flush() {
        unsafe {
            _idtr.size = 256 * size_of::<IDTEntry>() as u16 - 1;
            _idtr.base = &raw const GLOBAL_IDT as u32;

            asm!(
                "lidt [{}]",

                in(reg) (&raw const _idtr),
            );
        }
    }
}

pub fn isr_install() {
    IDT::set_gate(0,  _isr0,  0x08, 0x8E);
    IDT::set_gate(1,  _isr1,  0x08, 0x8E);
    IDT::set_gate(2,  _isr2,  0x08, 0x8E);
    IDT::set_gate(3,  _isr3,  0x08, 0x8E);
    IDT::set_gate(4,  _isr4,  0x08, 0x8E);
    IDT::set_gate(5,  _isr5,  0x08, 0x8E);
    IDT::set_gate(6,  _isr6,  0x08, 0x8E);
    IDT::set_gate(7,  _isr7,  0x08, 0x8E);
    IDT::set_gate(8,  _isr8,  0x08, 0x8E);
    IDT::set_gate(9,  _isr9,  0x08, 0x8E);
    IDT::set_gate(10, _isr10, 0x08, 0x8E);
    IDT::set_gate(11, _isr11, 0x08, 0x8E);
    IDT::set_gate(12, _isr12, 0x08, 0x8E);
    IDT::set_gate(13, _isr13, 0x08, 0x8E);
    IDT::set_gate(14, _isr14, 0x08, 0x8E);
    IDT::set_gate(15, _isr15, 0x08, 0x8E);
    IDT::set_gate(16, _isr16, 0x08, 0x8E);
    IDT::set_gate(17, _isr17, 0x08, 0x8E);
    IDT::set_gate(18, _isr18, 0x08, 0x8E);
    IDT::set_gate(19, _isr19, 0x08, 0x8E);
    IDT::set_gate(20, _isr20, 0x08, 0x8E);
    IDT::set_gate(21, _isr21, 0x08, 0x8E);
    IDT::set_gate(22, _isr22, 0x08, 0x8E);
    IDT::set_gate(23, _isr23, 0x08, 0x8E);
    IDT::set_gate(24, _isr24, 0x08, 0x8E);
    IDT::set_gate(25, _isr25, 0x08, 0x8E);
    IDT::set_gate(26, _isr26, 0x08, 0x8E);
    IDT::set_gate(27, _isr27, 0x08, 0x8E);
    IDT::set_gate(28, _isr28, 0x08, 0x8E);
    IDT::set_gate(29, _isr29, 0x08, 0x8E);
    IDT::set_gate(30, _isr30, 0x08, 0x8E);
    IDT::set_gate(31, _isr31, 0x08, 0x8E);
}
