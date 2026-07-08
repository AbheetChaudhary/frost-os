#![allow(unused)]

#![feature(abi_x86_interrupt)]

#![no_main]
#![no_std]

use core::arch::asm;

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

fn disable_interrupts() {
    unsafe {
        asm!(
            "cli"
        );
    }
}

fn enable_interrupts() {
    unsafe {
        asm!(
            "sti"
        );
    }
}

#[unsafe(no_mangle)]
fn kernel_main(_: *const (), magic_number: u32) -> ! {
    WRITER.lock().clear_screen();

    disable_interrupts();

    let mut gdt: gdt::GDT = Default::default();
    gdt.add_default_null_segment();
    gdt.add_default_code_segment();
    gdt.add_default_data_segment();

    gdt.gdt_flush();

    isr::IDT::flush();
    isr::isr_install();
    irq::irq_install();

    enable_interrupts();
    println!("IRQ/ISR setup completed, enabling interrupts...");

    timer::install();
    println!("Initialized timer.");

    keyboard::install();
    println!("Initialized keyboard.");

    loop {}
}

