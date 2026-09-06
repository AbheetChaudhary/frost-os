#![feature(sync_unsafe_cell)]
#![no_std]

#![warn(raw_borrows_via_references)]


use core::ffi::c_void;
use core::mem::MaybeUninit;

pub mod console;
pub mod serial;
pub mod uefi;
pub mod protocols;
pub mod elf;
pub mod acpi;
pub mod kinit;
pub mod mm;

pub mod kserial; // TODO: delete this.

/// The global system table that must be initialized before using any other
/// thing from this library.
pub static mut GLOBAL_ST: *mut uefi::SystemTable = core::ptr::null_mut();

/// The global boot services pointer that must be initialized before using any
/// other thing from this library.
pub static mut GLOBAL_BS: *mut uefi::BootServices = core::ptr::null_mut();

/// The global runtime services services pointer that must be initialized before
/// using any other thing from this library.
pub static mut GLOBAL_RT: *mut uefi::RuntimeServices = core::ptr::null_mut();

/// A pointer to global serial IO protocol. All writes to serial output will go
/// through this. If it is `None` then any attempts to write should be ignored.
pub static mut SERIAL_IO: Option<*mut uefi::SerialIoProtocol> = None;

// Template panic handler.
/*
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        println!("panic occured at {}:{}:{}", loc.file(), loc.line(), 
            loc.column());
    } else {
        println!("panic occured, could not get location");
    }

    let message = info.message();

    if let Some(s) = message.as_str() {
        println!("{}", s);
    } else {
        println!("{:?}", message);
    }

    loop {}
}
*/

/// A macro to create utf-16 strings from ASCII strings.
///
/// The returned array of `u16`'s is valid utf-16 string whose address could be
/// taken and passed into EFIAPI functions. Most of those functions expect null
/// terminated strings so the string literals passed to this macro usually have
/// an `\0` in the end.
///
/// It could be used to create a utf-16 string in any scope as `const` or
/// inside a function though `let`. The calling syntax differ in both cases.
///
/// # Usage
///
/// - Inside a function:
///
/// `let kernel_path = utf16!("\\EFI\\kernel.bin\0");`
///
/// - As `const`:
///
/// `utf16!(KERNEL_PATH, "\\EFI\\kernel.bin\0");`
///
/// This will put `const KERNEL_PATH: [u16; ...] = ...` in the current scope
/// with appropriate array length arguments filled in.
#[macro_export]
macro_rules! utf16 {
    ($string:literal) => {{
        let _s = $string;

        // The passed string must be ascii.
        assert!(_s.is_ascii());

        #[allow(non_upper_case_globals)]
        const _length: usize = $string.len();

        let mut _name: [u16; _length] = [0u16; _];

        let _bytes = _s.as_bytes();

        let mut _i = 0;
        while _i < _bytes.len() {
            // ASCII u8 values are trivially convertible to utf16 by just a
            // cast to u16.
            _name[_i] = _bytes[_i] as u16;
            _i += 1;
        }
        _name
    }};

    ($name:ident, $string:literal) => {
        const $name: [u16; $string.len()] = {
            let _s = $string;

            // The passed string must be ascii.
            assert!(_s.is_ascii());

            #[allow(non_upper_case_globals)]
            const _length: usize = $string.len();

            let mut _name: [u16; _length] = [0u16; _];

            let _bytes = _s.as_bytes();

            let mut _i = 0;
            while _i < _bytes.len() {
                // ASCII u8 values are trivially convertible to utf16 by just a
                // cast to u16.
                _name[_i] = _bytes[_i] as u16;
                _i += 1;
            }
            _name
        };
    }
}

/// The initializer for the entire library. This does things such as setting
/// the global `SystemTable` that will be used by a lot of functions. Doing
/// anything in this library without first calling this function in the
/// `efi_main` is going to give wrong results.
///
/// # Safety
///
/// - This function must be called exactly once, that too in the beginning of
///   `efi_main` before doing anything else.
///
/// - Must be used only with a single threaded program.
pub unsafe fn init_once_single_threaded(st: *mut uefi::SystemTable) {
    unsafe {
        GLOBAL_ST = st;

        let bs = &raw mut *(*st).boot_services;
        GLOBAL_BS = bs;

        let rt = &raw mut *(*st).runtime_services;
        GLOBAL_RT = rt;
    }
}

pub unsafe fn init_global_serial(serial: *mut uefi::SerialIoProtocol) {
    unsafe {
        SERIAL_IO = Some(serial);
    }
}

/// Helper struct to iterate over a null terminated `Char16` string.
pub struct Char16Iter {
    /// Pointer to the null terminated UCS2 string.
    ptr: *const uefi::Char16,

    /// Index of the next character we will yield, or the number of characters
    /// we have already yielded.
    idx: usize,
}

impl Char16Iter {
    /// Create a new `Char16Iter` from the given `*const Char16`.
    ///
    /// # Safety
    ///
    /// - `ptr` must be a valid pointer to a null terminated `Char16` string.
    pub unsafe fn from_char16_ptr(ptr: *const uefi::Char16) -> Char16Iter {
        Self {
            ptr,
            idx: 0,
        }
    }
}

impl Iterator for Char16Iter {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = self.ptr.map_addr(|addr| addr + self.idx * size_of::<u16>());

        let c = unsafe { ptr.read() };

        if c.0 != 0 {
            self.idx += 1;

            Some(c.0)
        } else {
            None
        }
    }
}

/// Helper struct to iterate over a `Char16` string with known `u16`length.
///
/// It keeps iterating even after finding a null character if it had not gone
/// through its entire length.
pub struct Char16IterN {
    ptr: *const uefi::Char16,

    idx: usize,
    
    len: usize,
}

impl Char16IterN {
    /// Create a new `Char16IterN` from the given `*const Char16`.
    ///
    /// # Safety
    ///
    /// - `ptr` must be a valid pointer to a `Char16` string with length `len`,
    /// without counting any terminal null byte.
    ///
    /// # Note
    ///
    /// The `len` is `u16` length, and not byte length.
    pub unsafe fn from_char16_ptr_and_len(ptr: *const uefi::Char16,
        len: usize) -> Char16IterN {

        Self {
            ptr,
            idx: 0,
            len,
        }
    }
}

impl Iterator for Char16IterN {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.len {
            return None;
        }

        let ptr =  self.ptr.map_addr(|addr| addr + self.idx * size_of::<u16>());
        self.idx += 1;

        let c16 = unsafe { ptr.cast::<u16>().read() };
        Some(c16)
    }
}

pub fn handle_protocol_strict(handle: uefi::Handle, guid: &uefi::Guid) -> *mut c_void {
    let mut proto_ptr: MaybeUninit<*mut c_void> = MaybeUninit::zeroed();
    
    unsafe {
        bs_call_strict!(handle_protocol(handle, guid, proto_ptr.as_mut_ptr()));
    }

    unsafe { proto_ptr.assume_init() }
}

pub unsafe fn allocate_pool_strict(pool_size: usize) -> *mut c_void {
    let mut pool_ptr: MaybeUninit<*mut c_void> = MaybeUninit::zeroed();

    unsafe {
        bs_call_strict!(allocate_pool(uefi::MemoryTypeBefore::LoaderData.into(),
            pool_size, pool_ptr.as_mut_ptr()));
    }

    unsafe { pool_ptr.assume_init() }
}

pub struct AddressMap {
    pub desc_ptr: *mut uefi::MemoryDescriptor,
    pub map_size: usize,
    pub desc_size: usize,
    pub map_key: usize,
    pub desc_version: u32,
}

impl Drop for AddressMap {
    fn drop(&mut self) {
        unsafe {
            bs_call_strict!(free_pool(self.desc_ptr.cast()));
        }
    }
}

/// Get memory map as a pointer to memory descriptor, number of descriptors,
/// descriptor size and map key.
pub fn get_memory_map_strict() -> AddressMap {
    // The number of memory maps that there might be.
    let guess_max_desc_count = 64;

    // Pool size required to hold guess_max_desc_count number of memory descriptors.
    let pool_size = guess_max_desc_count * size_of::<uefi::MemoryDescriptor>();

    // Pointer to the beginning of the allocated pool.
    let mut pool_ptr: MaybeUninit<*mut c_void> = MaybeUninit::zeroed();

    // Allocate enough memory to hold guess_max_desc_count number of memory descriptions.
    let r = unsafe {
        bs_call!(allocate_pool(uefi::MemoryTypeBefore::LoaderData.into(),
            pool_size, pool_ptr.as_mut_ptr()))
    };

    if r.is_not_success() {
        panic!("allocate_pool: {:?}", r);
    }

    // serial!("allocated pool\n");

    // The beginning of the buffer we allocated.
    let mut mmap_desc_ptr = unsafe {
        pool_ptr.assume_init().cast::<uefi::MemoryDescriptor>()
    };

    // The size of the buffer we allocated to receive the memory map.
    let mut mmap_size = pool_size;

    // These fields will be filled by a successful call to get_memory_map.
    let mut map_key: MaybeUninit<usize> = MaybeUninit::zeroed();
    let mut desc_size: MaybeUninit<usize> = MaybeUninit::zeroed();
    let mut desc_version: MaybeUninit<u32> = MaybeUninit::zeroed();

    let r = unsafe {
        bs_call!(get_memory_map(&mut mmap_size, mmap_desc_ptr, map_key.as_mut_ptr(),
            desc_size.as_mut_ptr(), desc_version.as_mut_ptr()))
    };

    if r.is_not_success() {
        // serial!("failed to get memory map, first attempt\n");

        // If the call failed because the allocated buffer was too small, then
        // try again.
        if r == uefi::Status::from(uefi::EfiStatus::BufferTooSmall) {
            // serial!("failure occured due to small pool\n");

            // First we free the allocated buffer.
            unsafe { bs_call_strict!(free_pool(mmap_desc_ptr.cast())); }

            // New required size to hold the memory map. This is double of the
            // required size. The required size was written to mmap_size by the
            // failed call to get_memory_map.
            let new_pool_size = mmap_size * 2;

            let mut new_pool_ptr: MaybeUninit<*mut c_void> = MaybeUninit::zeroed();

            // Allocate the new and bigger pool.
            let r = unsafe {
                bs_call!(allocate_pool(uefi::MemoryTypeBefore::LoaderData.into(),
                    new_pool_size, new_pool_ptr.as_mut_ptr()))
            };

            if r.is_not_success() {
                panic!("allocate_pool: {:?}", r);
            }

            mmap_desc_ptr = unsafe { new_pool_ptr.assume_init().cast() };
            mmap_size = new_pool_size;
        } else {
            panic!("get_memory_map: {:?}", r);
        }
    }

    // serial!("successfully resized pool\n");

    // Try to get the memory map again.
    let r = unsafe {
        bs_call!(get_memory_map(&mut mmap_size, mmap_desc_ptr, map_key.as_mut_ptr(),
            desc_size.as_mut_ptr(), desc_version.as_mut_ptr()))
    };

    if r.is_not_success() {
        panic!("get_memory_map: {:?}", r);
    }

    // serial!("successfully got memory map\n");

    let (map_key, desc_size, desc_version) = unsafe {
        (map_key.assume_init(), desc_size.assume_init(), desc_version.assume_init())
    };

    // let desc_count = mmap_size / desc_size;

    // serial!("map_key: {}, desc_size: {}, desc_version: {}, mmap_size: {}\n", map_key, desc_size, 
    //     desc_version, mmap_size);

    AddressMap {
        desc_ptr: mmap_desc_ptr,
        map_size: mmap_size,
        desc_size,
        map_key,
        desc_version,
    }
}

