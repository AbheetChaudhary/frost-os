#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use core::arch::asm;
use core::ffi::c_void;

/*

Runtime service:
- ResetSystem()


*/

/*
use loader::{
    uefi,
    acpi,
    utf16,
    protocols,
    println, serial, serialn,
    bs_call_strict, st_call_strict, rt_call_strict, rt_call, bs_call,
        proto_call_strict, proto_call,
    serial::{serial_write_raw, serialn_write_raw},
    elf,
    kinit,
};
*/

use loader::*;
use serial::{serialn_write_raw};

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        kserial!("\npanic occured at {}:{}:{}\n", loc.file(), loc.line(), 
            loc.column());
    } else {
        kserial!("\npanic occured, could not get location\n");
    }

    let message = info.message();

    if let Some(s) = message.as_str() {
        kserial!("{}\n", s);
    } else {
        kserial!("{:?}\n", message);
    }

    loop {}
}

pub fn debug_dump(phys_mem: &mm::PhysMem) {
    kserial!("----------------------PhysMem----------------------\n");
    kserial!("active region count: {}\n", phys_mem.active_region_count());

    for region in phys_mem.active_regions() {
        kserial!("{:?}({} pages, start: {:#x})\n", region.mem_type(),
        region.page_count(), region.first_addr());
    }

    kserial!("---------------------------------------------------\n");
}

#[unsafe(export_name = "efi_main")]
pub extern "efiapi" fn main(image: uefi::Handle, st: *mut uefi::SystemTable) -> ! {
    unsafe {
        // Initialize pointers to system table, boot services and runtime services.
        loader::init_once_single_threaded(st);

        // Initialize global serial.
        loader::init_global_serial(locate_serial_protocol());
    }

    serial!("serial initialized successfully...hello from serial!\n");

    serial!("hi there!\n");

    let mut time_ptr: MaybeUninit<*mut uefi::Time> = MaybeUninit::zeroed();

    unsafe {
        bs_call_strict!(allocate_pool(uefi::MemoryTypeBefore::LoaderData.into(),
            size_of::<uefi::Time>(), time_ptr.as_mut_ptr().cast()));
    }

    let time_ptr = unsafe { time_ptr.assume_init() };

    unsafe { rt_call_strict!(_get_time(time_ptr, core::ptr::null_mut())); }

    let time = unsafe { time_ptr.read() };

    serial!("Current time: {}-{}-{}, {}:{}:{}:{}\n", time.day, time.month,
        time.year, time.hour, time.minute, time.second, time.nano_second);

    let r = unsafe { bs_call!(free_pool(time_ptr.cast())) };
    if !r.is_success() {
        serial!("hello, world!\n");
    }

    // Print cr0.
    let cr0: usize;
    unsafe {
        asm!(r#"
                mov     {}, cr0
            "#,
            out(reg) cr0,
        );
    }
    serial!("cr0: 0x{:x}\n", cr0);

    // Print cr3.
    let cr3: usize;
    unsafe {
        asm!(r#"
                mov     {}, cr3
            "#,
            out(reg) cr3,
        );
    }
    serial!("cr3: 0x{:x}\n", cr3);

    // Global device path to text protocol for its utility functions.
    let device_path_to_text_proto = locate_protocol_strict(
        &protocols::EFI_DEVICE_PATH_TO_TEXT_PROTOCOL_GUID, 
    ).cast::<protocols::DevicePathToTextProtocol>();

    // Loaded image protocol from the image handle.
    let loaded_image_proto = handle_protocol_strict(
        image,
        &protocols::EFI_LOADED_IMAGE_PROTOCOL_GUID
    ).cast::<protocols::LoadedImageProtocol>();

    serial!("loaded image protocol: {:p}\n", loaded_image_proto);

    // Get the file name of the loaded image as string.
    let file_name_string = unsafe {
        // Get device path to text utility function.
        let device_path_to_text = (*device_path_to_text_proto).device_path_to_text;      

        let dev_path_proto = (*loaded_image_proto).file_path;

        device_path_to_text(dev_path_proto.cast_const(), false, false)
    };

    // Print file path of the loaded image.
    unsafe { serialn_write_raw(file_name_string) };

    // Device handle from which image was loaded.
    let dev_handle = unsafe { (*loaded_image_proto).device_handle };

    // Locate device path protocol on the device handle from which the image was
    // loaded.
    let img_dev_path_proto = handle_protocol_strict(
        dev_handle,
        &protocols::EFI_DEVICE_PATH_PROTOCOL_GUID,
    ).cast::<protocols::DevicePathProtocol>();

    // Get the device path of the loaded images' device.
    let dev_path_string = unsafe {
        // Get device path to text utility function.
        let device_path_to_text = (*device_path_to_text_proto).device_path_to_text;

        device_path_to_text(img_dev_path_proto.cast_const(), false, false)
    };

    // Print device path of the loaded image.
    unsafe { serialn_write_raw(dev_path_string) };

    unsafe {
        let image_base = (*loaded_image_proto).image_base;
        let image_size = (*loaded_image_proto).image_size;

        serialn!("image base: {:p}, image size: {}", image_base, image_size);
    }

    let kernel_file = kinit::load_kernel_file(dev_handle);

    let acpi = acpi_stuff();

    unsafe {
        kserial::SerialPort::init();
    }
    kserial!("this is from COM1 port, kserial is working.\n");

    let phys_mem = kinit::prepare_for_memory_takeover()
        .expect("takeover attempt failed");

    debug_dump(&phys_mem);

    unsafe {
        // This has drop glue that triggers boot services. Avoid it getting dropped.
        let address_map = get_memory_map_strict();

        bs_call_strict!(exit_boot_services(image, address_map.map_key));

        core::mem::forget(address_map);
    }

    kserial!("called exit boot services\n");
    
    kernel_file.start_kernel(phys_mem, acpi);

    /*
    // Eventually call the exit_boot_services().
    // uefi::EfiStatus::Success.into()
    unsafe {
        serialn!("shutting down...");
        shutdown(uefi::EfiStatus::Success.into())
    };
    */
}

/// Dump confituration tables from the global systemtable.
pub fn dump_config_table() {
    // Number of configuration table entries.
    let num_entries = unsafe { (*loader::GLOBAL_ST).num_of_table_entries };

    if num_entries == 0 {
        serialn!("Configuration table is empty");
        return;
    } else {
        serialn!("configuration table has {} entries", num_entries);
    }

    // Configuration table pointer.
    let config_table = unsafe { (*loader::GLOBAL_ST).config_table };

    for i in 0..num_entries {
        // Pointer to next config table entry.
        let ptr = config_table.map_addr(|addr| {
            addr + i * size_of::<uefi::ConfigurationTable>()
        });

        let ct = unsafe { ptr.read() };
        serialn!("{:?}", ct.vendor_guid);
    }
}

pub unsafe fn shutdown(status: uefi::Status) -> ! {
    unsafe {
        rt_call!(
            reset_system(
                uefi::EfiResetType::Shutdown.into(),
                status,
                0,
                core::ptr::null_mut()
            )
        );
    }
}


fn locate_protocol_strict(guid: &uefi::Guid) -> *mut c_void {
    let mut proto_ptr: MaybeUninit<*mut c_void> = MaybeUninit::zeroed();
    
    unsafe {
        bs_call_strict!(locate_protocol(guid, core::ptr::null(), proto_ptr.as_mut_ptr()));
    }

    unsafe { proto_ptr.assume_init() }
}

fn locate_serial_protocol() -> *mut uefi::SerialIoProtocol {
    let guid = uefi::EFI_SERIAL_IO_PROTOCOL_GUID;

    let serial_interface = locate_protocol_strict(&guid);

    serial_interface.cast()
}

fn _print_memory_map() {
    let addr_map = loader::get_memory_map_strict();

    let desc_count = addr_map.map_size / addr_map.desc_size;

    for i in 0..desc_count {
        // Pointer to the next descriptor.
        let ptr = addr_map.desc_ptr.map_addr(|addr| addr + i * addr_map.desc_size);

        let desc = unsafe { ptr.read() };

        core::hint::black_box(&desc);

        // Skip boot services code and data memory regions, as they will be
        // useless after we exit boot services.
        if desc.memory_type == uefi::MemoryTypeBefore::BootServicesCode.into()
            || desc.memory_type == uefi::MemoryTypeAfter::BootServicesData.into() {

            continue;
        }

        serial!("{:#?}\n", desc);
    }

    serial!("map_key: {}, desc_size: {}\n", addr_map.map_key, addr_map.desc_size);
}
