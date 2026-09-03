// #![allow(unused)]

// This tries to do iret/iretd by itself and can be hard to integrate with
// existing assembly code.
//
// #![feature(abi_x86_interrupt)]

#![feature(step_trait)]

#![no_main]
#![no_std]

use core::arch::asm;

pub mod vga_buffer;
pub mod utils;
pub mod isr;
pub mod irq;
pub mod timer;
// pub mod keyboard;
pub mod gdt;
pub mod cpu;
pub mod serial;
pub mod kore;
pub mod mm;

mod prelude;

use vga_buffer::WRITER;
use prelude::*;

/// A multiboot compliant bootloader identifies itself with this magic number.
const BOOTLDR_MAGIC: u32 = 0x2BADB002;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        serial!("kernel panicked at {}:{}:\n", location.file(), location.line());
    } else {
        serial!("kernel panicked:\n");
    }

    serial!("{}\n", info.message());

    halt_loop();
}

/// Halt the system.
pub fn halt_loop() -> ! {
    use core::arch::asm;

    unsafe {
        asm!(r#"
            2:
                cli
                hlt
                jmp 2b
            "#,

            options(noreturn),
        )
    }
}

#[allow(unused)]
fn blessed_machine() {
    serial!("machine spirit is appeased...\n");
}

// Trick to get RW- segment.
static mut FOO: usize = 0;

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn kernel_entry(phys_mem: mm::PhysMem) -> ! {

    /*
    core::hint::black_box(&raw mut FOO);

    unsafe { 
        serial::SerialPort::init();
    }

    serial!("this is the kernel speaking...!\n");

    loop {}
    */

    unsafe {
        cpu::disable_interrupts(); 
    }

    kmain(phys_mem);
}

/*
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_entry(mboot_info_raw: *const multiboot::RawInfo,
    magic_number: u32) -> ! {

    unsafe {
        cpu::disable_interrupts(); 

        mm::init();
    }

    let kmain_offset = kmain as *const () as usize + mm::HIGHER_BEGIN as usize;

    let kmain_fn_ptr: fn(*const multiboot::RawInfo, u32) -> ! = unsafe {
        core::mem::transmute(kmain_offset)
    };

    // kmain(mboot_info_raw, magic_number);

    // This is changin stack pointer, is it fine to do it from rust?
    unsafe {
        core::arch::asm!(
            "add    esp, 0xC0000000",
        );
    }

    kmain_fn_ptr(mboot_info_raw, magic_number);
}
*/

pub fn debug_dump(phys_mem: &mm::PhysMem) {
    serial!("----------------------PhysMem----------------------\n");
    serial!("active region count: {}\n", phys_mem.active_region_count());

    for region in phys_mem.active_regions() {
        serial!("{:?}({} pages)\n", region.mem_type(), region.page_count());
    }

    serial!("---------------------------------------------------\n");
}

#[unsafe(no_mangle)]
pub fn kmain(phys_mem: mm::PhysMem) -> ! {
    // Disable interrupts and install GDTR.
    unsafe { cpu::disable_interrupts(); }

    let mut gdt: gdt::GDT = Default::default();
    gdt.push_default_null_segment();
    gdt.push_default_code_segment();
    gdt.push_default_data_segment();

    gdt.gdt_install();

    // WRITER.lock().clear_screen();
    unsafe { 
        serial::SerialPort::init();
    }

    serial!("\n\n");
    serial!("################################################################\n");
    serial!("################            RESET            ###################\n");
    serial!("################################################################\n");
    serial!("\n");

    unsafe {
        isr::isr_install();
        irq::irq_install();
    }

    serial!("...installed ISR and IRQ's\n");

    timer::install();
    serial!("...installed timer\n");

    unsafe { cpu::enable_interrupts(); }

    serial!("...enabled interrupts\n");

    // keyboard::install();
    // serial!("...initialized keyboard\n");

    if cpu::has_cpuid() {
        serial!("...has CPUID=true\n");
        let extended_cpuid = unsafe { cpu::has_extended_cpuid() };
        serial!("...has extended CPUID={}\n", extended_cpuid);

        serial!("...has long mode={}\n", unsafe { cpu::has_long_mode() });
    } else {
        serial!("...has CPUID=false\n");
    }

    serial!("kernel_entry: {:p}\n", kernel_entry as *const u8);

    serial!("hello, world from Serial\n");

    serial!("kmain: {:p}\n", kmain as *const ());

    debug_dump(&phys_mem);

    serial!("going to loop...\n");
    serial!("ticks: {}\n", timer::get_current_ticks());

    // serial!("going to do a null dereference, should page fault!\n");
    // let _: u8 = unsafe { core::ptr::read_volatile(core::ptr::null()) };

    loop {

    }
}

