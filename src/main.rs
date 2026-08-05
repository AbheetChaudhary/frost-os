// #![allow(unused)]

// This tries to do iret/iretd by itself and can be hard to integrate with
// existing assembly code.
//
// #![feature(abi_x86_interrupt)]

#![no_main]
#![no_std]

pub mod vga_buffer;
pub mod utils;
pub mod isr;
pub mod irq;
pub mod timer;
pub mod keyboard;
pub mod gdt;
pub mod cpu;

use vga_buffer::*;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!("kernel panicked at {}:{}:", location.file(), location.line());
    } else {
        println!("kernel panicked:");
    }

    println!("{}", info.message());

    loop {}
}

#[unsafe(no_mangle)]
pub fn kernel_entry(_: *const (), magic_number: u32) -> ! {
    WRITER.lock().clear_screen();

    println!("machine spirit appeased...");
    println!("magic number: 0x{:x}", magic_number);

    unsafe { cpu::disable_interrupts(); }

    let mut gdt: gdt::GDT = Default::default();
    gdt.push_default_null_segment();
    gdt.push_default_code_segment();
    gdt.push_default_data_segment();

    gdt.gdt_install();

    unsafe { utils::mem_dump(gdt::get_gdtr_address().cast(), 36); }

    unsafe {
        isr::isr_install();
        irq::irq_install();
    }

    println!("...installed ISR and IRQ's");

    timer::install();
    println!("...installed timer");

    unsafe { cpu::enable_interrupts(); }

    println!("...enabled interrupts");

    // utils::sleep_secs(1);
    // println!("...slept for 1 second");

    // utils::sleep_millis(1000);
    // println!("...slept for 1000ms");

    // println!("IRQ/ISR setup completed, enabling interrupts...");

    // timer::install();
    // println!("Initialized timer.");

    keyboard::install();
    println!("...initialized keyboard");

    if cpu::has_cpuid() {
        println!("...has CPUID=true");
        let extended_cpuid = unsafe { cpu::has_extended_cpuid() };
        println!("...has extended CPUID={}", extended_cpuid);

        println!("...has long mode={}", unsafe { cpu::has_long_mode() });
    } else {
        println!("...has CPUID=false");
    }

    println!("kernel_entry: {:p}", kernel_entry as *const u8);
 
    loop {}
}

