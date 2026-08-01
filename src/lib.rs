// #![allow(unused)]

// This tries to do iret/iretd by itself and can be hard to integrate with
// existing assembly code.
//
// #![feature(abi_x86_interrupt)]

#![no_main]
#![no_std]

use core::arch::asm;

unsafe extern "C" {
    static mut _gdtr: gdt::GDTR;
}

pub mod vga_buffer;
pub mod utils;
pub mod isr;
pub mod irq;
pub mod timer;
pub mod keyboard;
pub mod gdt;

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

#[inline(always)]
unsafe fn disable_interrupts() {
    unsafe {
        asm!(
            "cli"
        );
    }
}

#[inline(always)]
unsafe fn enable_interrupts() {
    unsafe {
        asm!(
            "sti"
        );
    }
}

#[unsafe(no_mangle)]
fn kernel_main(_: *const (), magic_number: u32) -> ! {
    WRITER.lock().clear_screen();

    println!("machine spirit appeased...");
    println!("magic number: 0x{:x}", magic_number);

    unsafe { disable_interrupts(); }

    let mut gdt: gdt::GDT = Default::default();
    gdt.push_default_null_segment();
    gdt.push_default_code_segment();
    gdt.push_default_data_segment();

    gdt.gdt_install();

    unsafe { utils::mem_dump(&raw const _gdtr as *const u8, 36); }

    unsafe {
        isr::isr_install();
        irq::irq_install();
    }

    println!("...installed ISR and IRQ's");

    timer::install();
    println!("...installed timer");

    unsafe { enable_interrupts(); }

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

    loop {}
}
