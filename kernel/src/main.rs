#![allow(unused)]

// This tries to do iret/iretd by itself and can be hard to integrate with
// existing assembly code.
//
// #![feature(abi_x86_interrupt)]

#![feature(fn_align)]
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
pub mod address;
pub mod boot_mem;
pub mod mm;
pub mod acpi;
pub mod page_tables;
pub mod kinit;

mod prelude;

use vga_buffer::WRITER;
use prelude::*;

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

#[derive(Debug)]
pub struct AcpiHandover {
    xsdt: usize,
    apic: usize,
    hpet: usize,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_entry(phys_mem: *const boot_mem::PhysMem, 
    acpi: *const AcpiHandover) -> ! {

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

    let phys_mem = unsafe { phys_mem.read() };
    let acpi = unsafe { acpi.read() };

    kmain(phys_mem, acpi);
}

pub fn debug_dump(phys_mem: &boot_mem::PhysMem) {
    serial!("----------------------PhysMem----------------------\n");
    serial!("active region count: {}\n", phys_mem.active_region_count());

    for region in phys_mem.active_regions() {
        serial!("{:?}({} pages, start: {:#x})\n", region.mem_type(), region.page_count(),
            region.addr_range().first_addr());
    }

    serial!("---------------------------------------------------\n");
}

#[unsafe(no_mangle)]
pub fn kmain(phys_mem: boot_mem::PhysMem, acpi: AcpiHandover) -> ! {
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

    gdt.gdt_install();

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

    serial!("{:#?}\n", acpi);

    let apic_ptr = unsafe {
        core::ptr::with_exposed_provenance::<u8>(acpi.apic)
    };

    for i in 0..4 {
        let b = unsafe { apic_ptr.add(i).read() };
        serial!("{}", b as char);
    }
    serial!("\n");

    // serial!("trampoline address: {:#x}\n", kinit::sipi_trampoline as usize);

    let madt_ptr = core::ptr::with_exposed_provenance(acpi.apic);

    kinit::mp_init(madt_ptr);

    loop {}
}
