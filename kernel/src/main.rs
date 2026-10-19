#![feature(fn_align, allocator_ext)]

// This tries to do iret/iretd by itself and can be hard to integrate with
// existing assembly code.
//
// #![feature(abi_x86_interrupt)]

#![no_std]
#![no_main]

extern crate alloc;

pub mod spin;
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
pub mod kmalloc;
pub mod kmain;

mod prelude;

struct FakeAllocator;

/// #[global_allocator] is needed if linking with `alloc`. But we can't have a
/// global allocator yet.
///
/// Taken from [Link against `alloc` without global allocator]
/// (https://github.com/rust-lang/wg-allocators/issues/123#issuecomment-1982120619)
///
/// If any code tries to use global allocator then compilation will fail due to
/// undefined symbols. If nothing ever invokes the global allocator then this
/// will all be optimized out and deleted.
unsafe impl core::alloc::GlobalAlloc for FakeAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        unsafe extern "Rust" {
            fn fake_alloc_this_doesnt_exist(layout: core::alloc::Layout) -> *mut u8;
        }
        unsafe { fake_alloc_this_doesnt_exist(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        unsafe extern "Rust" {
            fn fake_dealloc_this_doesnt_exist(ptr: *mut u8, layout: core::alloc::Layout);
        }
        unsafe { fake_dealloc_this_doesnt_exist(ptr, layout) }
    }
}

#[global_allocator]
static _ALLOCATOR: FakeAllocator = FakeAllocator;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        serial!("kernel panicked at {}:{}:\n", location.file(), location.line());
    } else {
        serial!("kernel panicked:\n");
    }

    serial!("{}\n", info.message());

    cpu::halt();
}


#[allow(unused)]
fn blessed_machine() {
    serial!("machine spirit is appeased...\n");
}

#[repr(C)]
#[derive(Debug)]
pub struct AcpiHandover {
    xsdt: usize,
    apic: usize,
    hpet: usize,
}

#[unsafe(no_mangle)]
pub unsafe extern "cdecl" fn kernel_entry(phys_mem: *const boot_mem::PhysMem, 
    acpi: *const AcpiHandover) -> ! {

    unsafe {
        cpu::disable_interrupts(); 
    }

    let phys_mem = unsafe { phys_mem.read_unaligned() };
    let acpi = unsafe { acpi.read_unaligned() };

    bootstrap(phys_mem, acpi);
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

pub fn bootstrap(phys_mem: boot_mem::PhysMem, _acpi: AcpiHandover) -> ! {
    // Disable interrupts and install GDTR.
    unsafe { cpu::disable_interrupts(); }

    let mut gdt: gdt::GDT = Default::default();
    gdt.push_default_null_segment();
    gdt.push_default_code_segment();
    gdt.push_default_data_segment();

    gdt.gdt_install();

    unsafe { 
        serial::SerialPort::init();
    }

    serial!("\n\n");
    serial!("################################################################\n");
    serial!("################            FROSt            ###################\n");
    serial!("################################################################\n");
    serial!("\n");

    unsafe {
        isr::isr_install();
        irq::irq_install();
    }

    serial!("installed ISR and IRQ's\n");

    timer::install();
    serial!("installed timer\n");

    unsafe { cpu::enable_interrupts(); }
    serial!("enabled interrupts\n");

    serial!("kernel_entry: {:p}\n", kernel_entry as *const u8);
    serial!("machine spirit appeased...\n");

    kinit::init_all_cores(phys_mem);
}
