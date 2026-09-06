use core::fmt;
use core::ffi::c_void;
use core::ops::{RangeInclusive, Range};

/// Prepared from the spec. for [UEFI 2.10](https://uefi.org/specs/UEFI/2.10/)
///
/// More Resources At:
///
/// Common UEFI data types: Table 2.3

/// NOTE:
/// - Unless otherwise specified, UEFI uses a standard CCITT32 CRC algorithm
///   with a seed polynomial value of 0x04c11db7 for its CRC calculations.
/// 
/// - EFI_STATUS is defined to have UINTN type. But typical enumerated types
///   are INT32 or UINT32 integer based. So we will be using usize with
///   EFI_STATUS.
///

// TODO: 
// - SimpleTextInputProtocol
// - SimpleTextOutputProtocol
// - RuntimeServices
// - BootServices
// - ConfigurationTable

/// Signature found in the `BootServices`'s table header.
pub const EFI_BOOT_SERVICES_SIGNATURE: u64 = 0x56524553544f4f42;

/// Signature found in the `RuntimeServices`'s table header.
pub const EFI_RUNTIME_SERVICES_SIGNATURE: u64 = 0x56524553544e5552;

/// A flexible array type to replicate C's incomplete array type.
///
/// A struct field of this type marks the beginning of an array of `T`'s whose
/// length is known through some other means and is not a compile time known
/// value.
///
/// This type must be used as the last field of whatever struct it is a part of.
#[repr(transparent)]
pub struct FlexibleArray<T>(core::marker::PhantomData<T>);

/// Physical address type to be used with the EFI api.
///
/// This really is a `u64` and not a `usize`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PhysicalAddress(pub u64);

impl PhysicalAddress {
    /// Get the physical address as a number.
    pub fn as_addr(&self) -> u64 {
        self.0
    }
}

/// Virtual address type to be used with the EFI api.
///
/// This really is a `u64` and not a `usize`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtualAddress(pub u64);

impl VirtualAddress {
    /// Get the virtual address as a number.
    pub fn as_addr(&self) -> u64 {
        self.0
    }
}

/// A collection of related interfaces.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Handle(*mut ());

impl Handle {
    /// Create a new handle from the given pointer.
    pub const fn from_raw(raw: *mut ()) -> Handle {
        Self(raw)
    }

    /// Create a handle with a null pointer. This should not be used for
    /// anything other than passing a NULL handle to EFI API functions where
    /// NULL makes sense.
    pub const fn null() -> Handle {
        Self(core::ptr::null_mut())
    }

    /// Get the raw pointer from inside the handle.
    pub const fn as_raw(self) -> *mut () {
        self.0
    }
}

/// `usize` with only its high bit set.
const HIGH_BIT_SET: usize = 1 << (size_of::<usize>() * 8 - 1);

/// A placeholder never type for use with `efiapi` macro.
///
/// Using `efiapi` without any input args means it is unimplemented, so this
/// type is used for input and output arguments.
pub(crate) enum _Unimplemented {}

/// Creates an EFI function signature which takes the given `$input_args` as
/// input and returns `Status` as output.
///
/// For example: 
///
/// - `efiapi!(u32, bool)` expands to `unsafe extern "efiapi" fn(u32, bool) -> Status`
///
/// - `efiapi!(*mut Self, usize)` expands to 
///     `unsafe extern "efiapi" fn(*mut Self, bool) -> Status`
macro_rules! efiapi {
    () => {
        unsafe extern "efiapi" fn(_Unimplemented) -> _Unimplemented
    };

    ($($input_args:ty),+) => {
        unsafe extern "efiapi" fn($($input_args,)+) -> Status
    };

    ($($arg:ident: $input_args:ty),+) => {
        unsafe extern "efiapi" fn($($arg: $input_args,)+) -> Status
    };
}
pub(crate) use efiapi;

/*
/// Creates a nullable EFI function signature which takes the given
/// `$input_args` as input and returns `Status` as output.
///
/// In Rust, just like references and Box, function pointers could also not be
/// null. But sometimes FFI boundary involves working with nullable function
/// pointers. The best way to deal with those situations would be to put the
/// function pointer inside an `Option.`
///
/// This macro does the same thing as [`efiapi`](efiapi) macro, but it wraps the
/// function signature in an `Option`.
macro_rules! efiapi_nullable {
    ($($input_args:ty),+) => {
        Option<efiapi!($($input_args),+)>
    }
}
*/

/// Create GUID from raw parts, i.e. from one `u32`, two `u16` and eight `u8`'s.
#[macro_export]
macro_rules! guid_from_raw {
    ($data1:literal, $data2:literal, $data3:literal, 
        $data4_0: literal, $data4_1: literal, $data4_2: literal, $data4_3: literal,
        $data4_4: literal, $data4_5: literal, $data4_6: literal, $data4_7: literal) => {{
        
        Guid {
            data1: $data1,
            data2: $data2,
            data3: $data3,
            data4: [
                $data4_0, $data4_1, $data4_2, $data4_3,
                $data4_4, $data4_5, $data4_6, $data4_7,
            ],

        }
    }}
}

/// Helper macro to call a protocol function with a cleaner syntax.
///
/// Use of this macro must be wrapped inside `unsafe` block.
///
/// To use this macro one needs a `*mut` pointer to the protocol interface
/// instance, the function that is needed to be called and its arguments.
///
/// The called function must expect a mutable pointer to the protocol as its
/// first argument.
///
/// Example:
///
/// `proto_call!(serial.write(arg1, arg2, ...))`
///
/// Here `serial` is of type `*mut SerialIoProtocol` and `write` is one of the
/// functions of `SerialIoProtocol`. The `serial` expression is passed to
/// `write` as its first argument before `arg1`. So `arg1` is actually the
/// second argument.
#[macro_export]
macro_rules! proto_call {
    ($proto_ptr:ident.$func:ident($($arg:expr),* $(,)?)) => {
        ((*$proto_ptr).$func)($proto_ptr, $($arg,)*)
    };
}

/// Helper macro to call a protocol function with a cleaner syntax.
///
/// Use of this macro must be wrapped inside `unsafe` block.
///
/// # Panics
///
/// This macro panics if return value of the called protocol function is not
/// `EfiStatus::Success`.
///
/// See [proto_call](proto_call) for more example.
#[macro_export]
macro_rules! proto_call_strict {
    ($proto_ptr:ident.$func:ident($($arg:expr),* $(,)?)) => {
        let r = ((*$proto_ptr).$func)($proto_ptr, $($arg,)*);

        if !r.is_success() {
            panic!("{}: {:?}", stringify!($func), r);
        }
    };
}

/// Call a function from `SystemTable`.
///
/// This macro needs to be wrapped inside an `unsafe` block. It is also possible
/// to call a function from an interface field inside `SystemTable`, just use
/// the first argument as the field name for the interface instance.
///
/// Usage: 
///
/// `st_call_strict!(func_name(arg1, arg2, ...));`
///
/// `st_call_strict!(field_name, func_name(arg1, arg2, ...));`
///
/// In the second version, the `output_string` function is implicitly passed a
/// pointer to `con_out` instance as the first argument, then `ptr` as the
/// second argument.
///
/// # Safety
/// 
/// - Correct global initialization.
/// - The function pointer should not be null.
/// - Read the documentation of the function and make sure that the arguments
///   make sense.
///
/// # Panics
///
/// This panics if the return value of the function is not `EfiStatus::Success`.
#[macro_export]
macro_rules! st_call_strict {
    ($func:ident($($input_args:expr),* $(,)?)) => {
        let r = ((*$crate::GLOBAL_ST).$func)($($input_args,)*);

        if !r.is_success() {
            panic!("{}: {:?}", stringify!($func), r);
        }
    };

    ($field:ident, $func:ident($($input_args:expr),* $(,)?)) => {
        let r = ((*(*$crate::GLOBAL_ST).$field).$func)((*$crate::GLOBAL_ST).$field,
            $($input_args,)*);

        if !r.is_success() {
            panic!("{}: {:?}", stringify!($func), r);
        }
    };
}

/// Call a function from `SystemTable` and return the `Status`.
///
/// This macro needs to be wrapped inside an `unsafe` block. It is also possible
/// to call a function from an interface field inside `SystemTable`, just use
/// the first argument as the field name for the interface instance.
///
/// Usage: 
///
/// `let r = st_call!(func_name(arg1, arg2, ...));`
///
/// `let r = st_call!(con_out, output_string(ptr));`
///
/// In the second version, the `output_string` function is implicitly passed a
/// pointer to `con_out` instance as the first argument, then `ptr` as the
/// second argument.
///
/// # Safety
/// 
/// - Correct global initialization.
/// - The function pointer should not be null.
/// - Read the documentation of the function and make sure that the arguments
///   make sense.
#[macro_export]
macro_rules! st_call {
    ($func:ident($($input_args:expr),* $(,)?)) => {
        ((*$crate::GLOBAL_ST).$func)($($input_args,)*)
    };

    ($field:ident, $func:ident($($input_args:expr),* $(,)?)) => {
        ((*(*$crate::GLOBAL_ST).$field).$func)((*$crate::GLOBAL_ST).$field,
            $($input_args,)*)
    };
}

/// Call a function from `BootServices`.
///
/// This macro needs to be wrapped inside an `unsafe` block.
///
/// Usage: `bs_call_strict!(func_name(arg1, arg2...));`
///
/// # Safety
/// 
/// - Correct global initialization.
/// - The function pointer should not be null.
/// - Read the documentation of the function and make sure that the arguments
///   make sense.
///
/// # Panics
///
/// This panics if the return value of the function is not `EfiStatus::Success`.
#[macro_export]
macro_rules! bs_call_strict {
    ($func:ident($($input_args:expr),* $(,)?)) => {
        let r = ((*$crate::GLOBAL_BS).$func)($($input_args,)*);

        if !r.is_success() {
            panic!("{}: {:?}", stringify!($func), r);
        }
    }
}

/// Call a function from `BootServices` and return the `Status`.
///
/// This macro needs to be wrapped inside an `unsafe` block.
///
/// Usage: `let r = bs_call!(func_name(arg1, arg2...));`
///
/// # Safety
/// 
/// - Correct global initialization.
/// - The function pointer should not be null.
/// - Read the documentation of the function and make sure that the arguments
///   make sense.
#[macro_export]
macro_rules! bs_call {
    ($func:ident($($input_args:expr),* $(,)?)) => {
        ((*$crate::GLOBAL_BS).$func)($($input_args,)*)
    }
}

/// Call a function from `RuntimeServices`.
///
/// This macro needs to be wrapped inside an `unsafe` block.
///
/// Usage: `rt_call_strict!(func_name(arg1, arg2...));`
///
/// # Safety
/// 
/// - Correct global initialization.
/// - The function pointer should not be null.
/// - Read the documentation of the function and make sure that the arguments
///   make sense.
///
/// # Panics
///
/// This panics if the return value of the function is not `EfiStatus::Success`.
#[macro_export]
macro_rules! rt_call_strict {
    ($func:ident($($input_args:expr),* $(,)?)) => {
        let r = ((*$crate::GLOBAL_RT).$func)($($input_args,)*);

        if !r.is_success() {
            panic!("{}: {:?}", stringify!($func), r);
        }
    }
}

/// Call a function from `RuntimeServices` and return the `Status`.
///
/// This macro needs to be wrapped inside an `unsafe` block.
///
/// Usage: `let r = rt_call!(func_name(arg1, arg2...));`
///
/// # Safety
/// 
/// - Correct global initialization.
/// - The function pointer should not be null.
/// - Read the documentation of the function and make sure that the arguments
///   make sense.
#[macro_export]
macro_rules! rt_call {
    ($func:ident($($input_args:expr),* $(,)?)) => {
        ((*$crate::GLOBAL_RT).$func)($($input_args,)*)
    }
}

/// Wide character type.
///
/// This is just a wrapper over u16. Its main purpose is to differentiate
/// between u16 as a number and u16 as a wide character.
///
/// No other correctness checks are performed for whether it is a valid wide
/// character or not, so that is up to the user to ensure.
#[repr(transparent)]
pub struct Char16(pub u16);

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Time {
    pub year:        u16,
    pub month:       u8,
    pub day:         u8,
    pub hour:        u8,
    pub minute:      u8,
    pub second:      u8,
    __pad1:          u8,
    pub nano_second: u32,
    pub timezone:    i16,
    pub daylight:    u8,
    __pad2:          u8,
}

/// Status code returned by EFI interface functions.
///
/// The main EFI spec. returns codes from `EfiStatus` enum. OEMs could have
/// their own status codes that could be different from the main spec. codes.
/// All EFI spec. functions return value of this type which may or may not be
/// equal to a variant of `EfiStatus`.
///
/// Both `Status` and `EfiStatus` are backed by `usize`. All `EfiStatus` codes
/// are valid `Status` codes, but not the other way around.
///
/// Error codes have their high bits set, whereas warning and success codes have
/// their high bits clear.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct Status(usize);

impl core::fmt::Debug for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // If it is one of the main EFI spec. status codes then print it as
        // such, otherwise print it slightly differently.

        if let Ok(efi_status) = EfiStatus::try_from(*self) {
            write!(f, "{:?}", efi_status)
        } else {
            write!(f, "Status(0x{:x})", self.0)
        }
    }
}

impl From<EfiStatus> for Status {
    fn from(val: EfiStatus) -> Self {
        Self(val as usize)
    }
}

impl PartialEq<EfiStatus> for Status {
    fn eq(&self, other: &EfiStatus) -> bool {
        *self == Self::from(*other)
    }
}

impl Status {
    /// Create a new `Status` which is same as the `EfiStatus::Success` variant.
    pub fn success() -> Status {
        Self(EfiStatus::Success as usize)
    }

    /// Checks whether the given status code is valid error EFI status. All EFI
    /// error status codes have their high bit set.
    ///
    /// Returns `None` if `self` is not even an `EfiStatus` variant.
    pub fn is_efi_error(&self) -> Option<bool> {
        let efi_status = EfiStatus::try_from(*self).ok()?;

        Some(efi_status.is_error())
    }

    /// Checks whether `self` is EfiStatus::Success.
    ///
    /// Here `false` return value does not mean failure, it could be a warning
    /// or some other kind of status code as well.
    pub fn is_success(&self) -> bool {
        // We don't need to first check whether it is a valid `EfiStatus`. If it
        // is 0 then it is a valid `EfiStatus` and it is the `Success` variant.
        self.0 == 0
    }

    /// Checks whether `self` is not success. If `self` is not success then it
    /// doesn't mean that it is a valid failure or warning.
    ///
    /// This function is helpful in error handling. The number of possible
    /// errors is too large, so it is not always easy to check we have a correct
    /// error code. But the success code is well known, we could just check for
    /// that, instead of asking what error/warning code we got.
    pub fn is_not_success(&self) -> bool {
        !self.is_success()
    }

    /// Creates a new `Status` from a variant of `EfiStatus`.
    pub fn efi_status(val: EfiStatus) -> Status {
        Self(val as usize)
    }

    /// Checks whether `self` is a valid `EfiStatus` variant.
    pub fn is_efi_status(&self) -> bool {
        EfiStatus::try_from(*self).is_ok()
    }

    /// Get the underlying `usize` status code.
    pub fn as_usize(&self) -> usize {
        self.0
    }
}

/// UEFI main spec. status codes.
///
/// For more information, read appendix D. Status Code, UEFI Spec. version 2.10
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum EfiStatus {
    Success = 0x0,

    // Error Codes
    LoadError = (0x1 | HIGH_BIT_SET),
    InvalidParameter,
    Unsupported,
    BadBufferSize,
    BufferTooSmall,
    NotReady,
    DeviceError,
    WriteProtected,
    OutOfResources,
    VolumeCorrupted,
    VolumeFull,
    NoMedia,
    MediaChanged,
    NotFound,
    AccessDenied,
    NoResponse,
    NoMapping,
    Timeout,
    NotStarted,
    AlreadyStarted,
    Aborted,
    ICMPError,
    TFTPError,
    ProtocolError,
    IncompatibleVersion,
    SecurityViolation,
    CRCError,
    EndOfMedia = (28 | HIGH_BIT_SET),
    EndOfFile = (31 | HIGH_BIT_SET),
    InvalidLanguage,
    CompromisedData,
    IPAddressConflict,
    HTTPError,

    // Warning Codes
    WarnUnknownGlyph = 1,
    WarnWriteFailure,
    WarnDeleteFailure,
    WarnBufferTooSmall,
    WarnStaleData,
    WarnFileSystem,
    WarnResetRequired,
}

impl TryFrom<Status> for EfiStatus {
    type Error = ();

    fn try_from(val: Status) -> Result<Self, Self::Error> {
        // Valid EfiStatus ranges.

        let success = EfiStatus::Success as usize;

        // One continuous range of values from load_error to end_of_media.
        let load_error   = EfiStatus::LoadError as usize;
        let end_of_media = EfiStatus::EndOfMedia as usize;
        let r1 = RangeInclusive::new(load_error,end_of_media);

        // Another continuous range of values from end_of_file to http_error.
        let end_of_file = EfiStatus::EndOfFile as usize;
        let http_error  = EfiStatus::HTTPError as usize;
        let r2 = RangeInclusive::new(end_of_file, http_error);

        // Another continuous range of values from warn_unknown_glyph to
        // warn_reset_required.
        let warn_unknown_glyph  = EfiStatus::WarnUnknownGlyph as usize;
        let warn_reset_required = EfiStatus::WarnResetRequired as usize;
        let r3 = RangeInclusive::new(warn_unknown_glyph, warn_reset_required);

        // If status falls in any of these ranges then it is a valid EfiStatus.
        let status = val.0;

        if status == success 
            || r1.contains(&status) 
            || r2.contains(&status)
            || r3.contains(&status)
        {
            // SAFETY: we have successfully verified that the status code falls
            // in the range of meaningful EfiStatus variants.
            Ok(unsafe { core::mem::transmute(status) })
        } else {
            Err(())
        }
    }
}

impl EfiStatus {
    /// Checks whether the given status is an error.
    pub fn is_error(&self) -> bool {
        (self.as_usize() & HIGH_BIT_SET) != 0
    }

    /// Checks whether the given status is a warning.
    pub fn is_warn(&self) -> bool {
        // It is error if the high bit is clear and it is not a Status::Success.
        self.as_usize() != 0 && (self.as_usize() & HIGH_BIT_SET) == 0
    }

    /// Get the underlying `usize` status code.
    pub fn as_usize(&self) -> usize {
        *self as usize
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Fourth chunk in GUID string.
        let fourth: [u8; 2] = self.data4[..2].try_into().unwrap();
        let fourth = u16::from_be_bytes(fourth);

        // Fifth chunk in GUID string, in big endign.
        let fifth: u64 = u64::from_be_bytes(self.data4) & 0x0000FFFFFFFFFFFF;

        // Last two chunks are printed in big endian, other chunks are printed
        // in little endian.

        write!(f, "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}", self.data1, self.data2,
            self.data3, fourth, fifth)
    }
}

/// All standard table types of UEFI are preceded by a `TableHeader`. It
/// includes a signature that is unique to the table type, any revision
/// information, and 32-bit CRC to validata the contents.
#[derive(Debug)]
#[repr(C)]
pub struct TableHeader {
    /// 32-bit signature that identifies the table that follows.
    signature: u64,

    /// The revision of the UEFI spec. to which this table conforms. It has
    /// 16-bits of major revision number followed by 16-bits of binary coded
    /// decimal lower revision number. Lower revision number goes from 00 to 99.
    ///
    /// Example: (Major Revision).(Minor Revision Upper Decimal).(Minor Revision
    ///     Lower Decimal)
    revision: u32,

    /// Size in bytes of the entire table, including the table header.
    header_size: u32,

    /// 32-bit CRC for the entire table. This is calculated by setting this
    /// field to zero and computing the 32-bit CRC of `header_size` many bytes.
    crc32: u32,

    /// Reserved field that must be set to zero.
    __reserved: u32,
}

/// Contains pointers to runtime and boot services tables.
///
/// Except the table header, all elements in the table header are pointers to
/// functions, either boot services or runtime services.
///
/// Prior to a call to `BootServices.ExitBootServices()`, all the fields in the
/// system table are valid, but once an operating system has taken control
/// through `ExitBootServices()`, only the `header`, `firmware_vendor`,
/// `firmware_revision`, `runtime_services`, `num_of_table_entries`, and
/// `config_table` are valid.
#[repr(C)]
pub struct SystemTable {
    /// Table header for `SystemTable`.
    pub header: TableHeader,

    /// A pointer to null terminated string that identifies the vendor that
    /// produced the system firmware of the platform.
    pub firmware_vendor: *const u16,

    /// A firmware vendor specific value that identifies the revision of the
    /// system firmware for the platform.
    pub firmware_revision: u32,

    /// The handle for active console input device. This handle must support
    /// `SimpleTextInputProtocol` and `SimpleTextInputExProtocol`. If there is
    /// no active console, these protocols must be still present.
    pub console_in_handle: Handle,

    /// A pointer to the `SimpleTextInputProtocol` interface that is associated
    /// with the `console_in_handle`.
    pub con_in: *mut SimpleTextInputProtocol,

    /// The handle for active console output device. This handle must support
    /// `SimpleTextOutputProtocol`. If there is no active console, this protocol
    /// must still be present.
    pub console_out_handle: Handle,

    /// A pointer to the `SimpleTextOutputProtocol` interface that is associated
    /// with `console_out_handle`.
    pub con_out: *mut SimpleTextOutputProtocol,

    /// Standard error handle for active standard error console device. This
    /// handle must support `SimpleTextOutputProtocol`. If there is no active
    /// console, this protocol must still be present.
    pub std_err_handle: Handle,

    /// A pointer to the `SimpleTextOutputProtocol` associated with
    /// `std_err_handle`.
    pub std_err: *mut SimpleTextOutputProtocol,

    /// A pointer to EFI Runtime Services table.
    pub runtime_services: *mut RuntimeServices,

    /// A pointer to EFI Boot Services table.
    pub boot_services: *mut BootServices,

    /// Number of system configuration tables in `config_table` buffer.
    pub num_of_table_entries: usize,

    /// A pointer to the system configuration tables.
    pub config_table: *const ConfigurationTable,
}

/// A table header and pointers to all boot services.
///
/// The function pointers in this struct are not valid after the OS has taken
/// control after a call to `exit_boot_services`.
///
/// # Note
///
/// Following fields are all function pointers, their signatures is currently
/// wrong, so fix that before using any of these. After fixing, remove the
/// prefix underscores and make the fields `pub`.
#[repr(C)]
pub struct BootServices {
    /// Table header for `BootServices` which contains the
    /// `EFI_BOOT_SERVICES_SIGNATURE`.
    pub header: TableHeader,

    /// Raises the task priority level.
    _raise_tpl: efiapi!(),

    /// Restores/lowers the task priority level.
    _restore_tpl: efiapi!(),

    /// Allocates pages of a particular type.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.AllocatePages()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-allocatepages)
    pub allocate_pages: efiapi!(AllocateType, MemoryType, usize, *mut PhysicalAddress),

    /// Free allocated pages.
    ///
    /// See 
    /// [EFI_BOOT_SERVICES.FreePages()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-freepages)
    pub free_pages: efiapi!(PhysicalAddress, usize),

    /// Returns the current boot services memory map and memory map key.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.GetMemoryMap()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-getmemorymap)
    pub get_memory_map: efiapi!(*mut usize, *mut MemoryDescriptor, *mut usize,
        *mut usize, *mut u32),

    /// Allocates a pool of a particular type. All allocations are eight bytes
    /// aligned.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.AllocatePool()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-allocatepool)
    pub allocate_pool: efiapi!(MemoryType, usize, *mut *mut c_void),

    /// Frees allocated pool.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.FreePool()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-freepool)
    pub free_pool: efiapi!(*mut c_void),

    /// Creates a general purpose event structure.
    _create_event: efiapi!(),

    /// Sets an event timer to be signaled at a particular time.
    _set_timer: efiapi!(),

    /// Stops execution until an event is signaled.
    _wait_for_event: efiapi!(),

    /// Signals an event.
    _signal_event: efiapi!(),

    /// Closes and frees an event structure.
    _close_event: efiapi!(),

    /// Checks whether an event is in the signaled state.
    _check_event: efiapi!(),

    /// Installs a protocol interface on a device handle.
    _install_protocol_interface: efiapi!(),

    /// Reinstalls a protocol interface on a device handle.
    _reinstall_protocol_interface: efiapi!(),

    /// Removes a protocol interface from a device handle.
    _uninstall_protocol_interface: efiapi!(),

    /// Queries a handle to determine if it supports the specified protocol.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.HandleProtocol()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-handleprotocol)
    pub handle_protocol: efiapi!(Handle, *const Guid, *mut *mut c_void),

    /// Reserved field, must be null.
    __reserved: *const c_void,

    /// Registers an event that is to be signaled whenever an interface is
    /// installed for a specified protocol.
    _register_protocol_notify: efiapi!(),

    /// Returns an array of handles that support the specified protocol.
    _locate_handle: efiapi!(),

    /// Locates all devices on a device path that support the specified protocol
    /// and returns the handle to device that is closest to the path.
    _locate_device_path: efiapi!(),

    /// Adds, updates, or removes a configuration table from the EFI System
    /// Table.
    _install_config_table: efiapi!(),

    /// Loads an EFI image into memory.
    _load_image: efiapi!(),

    /// Transfers control to the loaded image's entry point.
    _start_image: efiapi!(),

    /// Exits the image's entry point.
    _exit: efiapi!(),

    /// Unloads an image.
    _unload_image: efiapi!(),

    /// Terminates boot services.
    pub exit_boot_services: efiapi!(Handle, usize),

    /// Returns a monotonically increasing count for the platform.
    _get_next_monotonic_count: efiapi!(),

    /// Stalls the processor.
    _stall: efiapi!(),

    /// Resets and sets a watchdog timer used during the boot services time.
    _set_watchdog_timer: efiapi!(),

    /// Uses a set of precedence rules to find the best set of drivers to manage
    /// a controller.
    _connect_controller: efiapi!(),

    /// Informs a set of drivers to stop managing a controller.
    _disconnect_controller: efiapi!(),

    /// Adds elements to the list of agents consuming a protocol interface.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.OpenProtocol()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-openprotocol)
    pub open_protocol: efiapi!(Handle, *const Guid, *mut *mut c_void, Handle, Handle, u32),

    /// Removes elements from the list of agents consuming a protocol interface.
    _close_protocol: efiapi!(),

    /// Retrieve a list of agents that are currently consuming a protocol
    /// interface.
    _open_protocol_info: efiapi!(),

    /// Retrieves a list of protocols installed on a handle. The return buffer
    /// is automatically allocated.
    _protocols_per_handle: efiapi!(),

    /// Retrieves a list of handles from the handle database that meet the
    /// search criteria. The return buffer is automatically allocated.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.LocateHandleBuffer()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-locatehandlebuffer)
    pub locate_handle_buffer: efiapi!(LocateSearchType, *const Guid, *const c_void,
        *mut usize, *mut *mut Handle),

    /// Finds the first handle in the handle database that supports the
    /// specified protocol.
    ///
    /// See
    /// [EFI_BOOT_SERVICES.LocateProtocol()](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html#efi-boot-services-locateprotocol)
    pub locate_protocol: efiapi!(*const Guid, *const c_void, *mut *mut c_void),

    /// Installs one or more protocol interfaces onto a handle.
    _install_multiple_protocol_interface: efiapi!(),

    /// Removes one or more protocol interfaces from a handle.
    _uninstall_multiple_protocol_interface: efiapi!(),

    /// Computes and returns the 32-bit CRC value for the specified buffer.
    _calculate_crc32: efiapi!(),

    /// Copies the contents of one buffer to another.
    _copy_mem: efiapi!(),

    /// Fills a buffer with a specified value.
    _set_mem: efiapi!(),

    /// Creates an event structure as part of an event group.
    _create_event_ex: efiapi!(),
}

/// Memory types and their meanings before the call to `exit_boot_services()`.
///
/// Their meaning changes after the call.
#[repr(u32)]
pub enum MemoryTypeBefore {
    /// Not usable.
    ReservedMemoryType = 0,

    /// The code portion of a loaded UEFI application.
    LoaderCode,

    /// The data portion of a loaded UEFI application, and the default data
    /// allocation type used by UEFI application to allocate pool memory.
    LoaderData,

    /// The code portion of a loaded UEFI boot services driver.
    BootServicesCode,

    /// The data portions of a loaded UEFI Boot Services driver, and the default
    /// data allocation type used by a UEFI Boot Services driver to allocate
    /// pool memory.
    BootServicesData,

    /// Code portions of a loaded UEFI Runtime Driver.
    RuntimeServicesCode,

    /// The data portions of a loaded UEFI Runtime Driver, and the default
    /// data allocation type used by a UEFI Runtime Driver to allocate pool
    /// memory.
    RuntimeServicesData,

    /// Free(unallocated) memory.
    ConventionalMemory,

    /// Memory in which errors have been detected.
    UnusableMemory,

    /// Memory that holds the ACPI tables.
    AcpiReclaimMemory,

    /// Address space reserved for use by the firmware.
    AcpiMemoryNvs,

    /// Used by system firmware to request that a memory-mapped IO region be
    /// mapped by the OS to a virtual address so it can be accessed by EFI
    /// runtime services.
    MemoryMappedIo,

    /// System memory-mapped IO region that is used to translate memory cycles
    /// to IO cycles by the processor.
    ///
    /// This has special behavior for Itanium based processors, so check the
    /// spec. for that.
    MemoryMappedIoPortSpace,

    /// Address space reserved by the firmware for code that is part of the
    /// processor.
    PalCode,

    /// A memory region that operates as `ConventionalMemory`. However, it
    /// happens to also support byte-addressable non-volatility.
    PersistentMemory,

    /// A memory region that represents unaccepted memory, that must be accepted
    /// by the boot target before it can be used. 
    ///
    /// Unless otherwise noted, all other EFI memory types are accepted. For
    /// platforms that support unaccepted memory, all unaccepted valid memory
    /// will be reported as unaccepted in the memory map. Unreported physical
    /// address ranges must be treated as not-present memory.
    UnacceptedMemoryType,

    _MaxMemoryType,
}

/// Memory types and their meanings after the call to `exit_boot_services()`.
///
/// An image that calls `exit_boot_services()` first calls `get_memory_map()` to
/// obtain the current memory map. After the `exit_boot_services()` call the
/// image implicitly owns all unused memory in the map. This includes
/// `LoaderCode`, `BootServicesCode`, `BootServicesData`, and
/// `ConventionalMemory`. 
///
/// The OS loader and the OS must preserve any memory marked
/// `RuntimeServicesCode` and `RuntimeServicesData`.
#[repr(u32)]
pub enum MemoryTypeAfter {
    /// Not usable.
    ReservedMemoryType = 0,

    /// The UEFI OS loader or the OS may use this memory as they see fit. The
    /// loader that called `exit_boot_services()` may be utilizing more than one
    /// `LoaderCode` ranges.
    LoaderCode,

    /// The UEFI OS loader or the OS may use this memory as they see fit. The
    /// loader that called `exit_boot_services()` may be utilizing more than one
    /// `LoaderData` ranges.
    LoaderData,

    /// Memory available for general use.
    BootServicesCode,

    /// Memory available for general use.
    BootServicesData,

    /// The memory in this range must be preserved by the UEFI OS loader and OS
    /// in the working and ACPI S1-S3 states.
    RuntimeServicesCode,

    /// The memory in this range must be preserved by the UEFI OS loader and OS
    /// in the working and ACPI S1-S3 states.
    RuntimeServicesData,

    /// Memory available for general use.
    ConventionalMemory,

    /// Memory that contains errors and is not to be used.
    UnusableMemory,

    /// This memory is to be preserved by the OS loader or the OS until ACPI is
    /// enable. Once it is enabled the memory in this range is available for
    /// general use.
    ACPIReclaimMemory,

    /// This memory is to be preserved by the OS loader and the OS in working
    /// and ACPI S1-S3 states.
    ACPIMemoryNVS,

    /// This memory is not used by the OS. All system memory-mapped IO
    /// information should come from ACPI tables.
    MemoryMappedIO,

    /// This memory is not used by the OS. All system memory-mapped IO port
    /// space information should come from ACPI tables.
    MemoryMappedIOPortSpace,

    /// This memory is to be preserved by the OS loader or the OS in working and
    /// ACPI S1-S3 states. This memory might have other attributes too, defined
    /// by the processor implementation.
    PalCode,

    /// A memory region that operates as `ConventionalMemory`. However it also
    /// supports byte-addressable non-volatility.
    PersistentMemory,

    /// A memory region that represents unaccepted memory, that must be accepted
    /// by the boot target before it can be used. 
    ///
    /// Unless otherwise noted, all other EFI memory types are accepted. For
    /// platforms that support unaccepted memory, all unaccepted valid memory
    /// will be reported as unaccepted in the memory map. Unreported physical
    /// address ranges must be treated as not-present memory.
    UnacceptedMemoryType,

    _MaxMemoryType,
}

/// Memory types used with memory management functions.
///
/// With private fields and lack of any constructors, it must be created from
/// either `MemoryTypeBefore` or `MemoryTypeAfter`.
///
/// Certain memory types are reserved for use by the OEMs or OS vendors. So all
/// variants of `MemoryTypeBefore` and `MemoryTypeAfter` are valid values of
/// `MemoryType`, but the reverse is not true.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct MemoryType(u32);

impl fmt::Debug for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            0  => write!(f, "ReservedMemoryType"),
            1  => write!(f, "LoaderCode"),
            2  => write!(f, "LoaderData"),
            3  => write!(f, "BootServicesCode"),
            4  => write!(f, "BootServicesData"),
            5  => write!(f, "RuntimeServicesCode"),
            6  => write!(f, "RuntimeServicesData"),
            7  => write!(f, "ConventionalMemory"),
            8  => write!(f, "UnusableMemory"),
            9  => write!(f, "ACPIReclaimMemory"),
            10 => write!(f, "ACPIMemoryNVS"),
            11 => write!(f, "MemoryMappedIO"),
            12 => write!(f, "MemoryMappedIOPortSpace"),
            13 => write!(f, "PalCode"),
            14 => write!(f, "PersistentMemory"),
            15 => write!(f, "UnacceptedMemoryType"),

            // 16 => write!(f, "MaxMemoryType"),

            x => write!(f, "MemoryType({} /* unknown */)", x)

        }
    }
}

impl From<EfiMemoryType> for MemoryType {
    fn from(val: EfiMemoryType) -> Self {
        Self(val as u32)
    }
}

impl From<MemoryTypeBefore> for MemoryType {
    fn from(val: MemoryTypeBefore) -> Self {
        // This depends on whether the enum variants use 32-bit integers like
        // `MemoryType` or not. They do!
        Self(val as u32)
    }
}

impl From<MemoryTypeAfter> for MemoryType {
    fn from(val: MemoryTypeAfter) -> Self {
        // This depends on whether the enum variants use 32-bit integers like
        // `MemoryType` or not. They do!
        Self(val as u32)
    }
}

#[repr(u32)]
pub enum EfiMemoryType {
    ReservedMemoryType = 0,
    LoaderCode,
    LoaderData,
    BootServicesCode,
    BootServicesData,
    RuntimeServicesCode,
    RuntimeServicesData,
    ConventionalMemory,
    UnusableMemory,
    ACPIReclaimMemory,
    ACPIMemoryNVS,
    MemoryMappedIO,
    MemoryMappedIOPortSpace,
    PalCode,
    PersistentMemory,
    UnacceptedMemoryType,

    _MaxMemoryType,
}

impl TryFrom<MemoryType> for EfiMemoryType {
    type Error = ();

    fn try_from(other: MemoryType) -> Result<Self, Self::Error> {
        let reserve_memory_type = EfiMemoryType::ReservedMemoryType as u32;
        let _max_memory_type = EfiMemoryType::_MaxMemoryType as u32;

        let r = Range { start: reserve_memory_type, end: _max_memory_type };

        if r.contains(&other.0) {
            let x = unsafe { core::mem::transmute::<MemoryType, Self>(other) };
            Ok(x)
        } else {
            Err(())
        }
    }
}

/// Allocation types to be used with `allocate_pages()`.
#[repr(u32)]
pub enum AllocateType {
    /// Allocates any available range of pages that satisfy the request. On
    /// input, the address pointed by `memory` argument to `allocate_pages()` is
    /// ignored.
    AllocateAnyPages = 0,

    /// Allocates any available range of pages whose upper address is less than
    /// the input address provided in `memory` argument.
    AllocateMaxAddress,

    /// Allocate pages at the address pointed to by `memory` on input.
    AllocateAddress,
}

/// Pointers to all runtime services.
///
/// Unlike the `BootServices` table, the function pointers in this struct remain
/// valid even after the OS loader or the OS have taken control by calling
/// `BootServices::exit_boot_services`.
///
/// If a all to `set_virtual_address_map()` is made by the OS then the function
/// pointers in the table are fixed up to point to the new virtually mapped
/// entry points.
#[repr(C)]
pub struct RuntimeServices {
    pub header: TableHeader,

    /// Returns the current time and date and the time keeping capabilities of
    /// the platform.
    pub _get_time: efiapi!(time: *mut Time, _ptr: *mut ()),

    /// Sets the current local time and date information.
    _set_time: efiapi!(),

    /// Returns the current wakeup alarm clock setting.
    _get_wakeup_time: efiapi!(),

    /// Sets the system wakeup alarm clock time.
    _set_wakeup_time: efiapi!(),

    /// Used by UEFI OS loader to convert from physical addressing to virtual
    /// addressing.
    pub set_virtual_address_map: efiapi!(usize, usize, u32, *mut MemoryDescriptor),

    /// Used by EFI components to convert internal pointers when switching to
    /// virtual addresses.
    pub convert_pointer: efiapi!(usize, *mut *mut c_void),

    /// Returns the value of a variable.
    _get_variable: efiapi!(),

    /// Enumerates the current variable names.
    _get_next_variable_name: efiapi!(),

    /// Sets the value of a variable.
    _set_variable: efiapi!(),

    /// Returns the next high 32-bits of the platform's monotonic counter.
    _get_next_high_monotonic_count: efiapi!(),

    /// Resets the entire platform. This function does not returns.
    pub reset_system: unsafe extern "efiapi" fn(ResetType, Status, usize,
        *const c_void) -> !,

    /// Passes capsules to the firmware with both physical and virtual mapping.
    _update_capsule: efiapi!(),

    /// Returns if the capsule can be supported via `update_capsule`.
    _query_capsule_capabilities: efiapi!(),

    /// Returns information about the EFI variable store.
    _query_variable_info: efiapi!(),
}

/// `config_table` field in the `SystemTable`.
///
/// It is a pair of GUID and pointer for each available configuration table.
/// Each available configuration table is allowed at most one entry in the
/// `SystemTable`'s `config_table` field.
#[repr(C)]
pub struct ConfigurationTable {
    /// The 128-bit GUID that uniquely identifies the system configuration
    /// table.
    pub vendor_guid: Guid,

    /// The physical or virtual address of the table. It depends on the
    /// `vendor_guid` value whether it is physical or virtual and gets remapped
    /// during a call to `set_virtual_address_map()`.
    pub vendor_table: *mut c_void,
}

/// ACPI 2.0 or newer should use this GUID. The addresses referenced in this
/// table will be physical addresses and will not be fixed up when transitioning
/// from preboot to runtime phase.
pub const EFI_ACPI_TABLE_GUID: Guid = guid_from_raw!{
    0x8868e871,0xe4f1,0x11d3,
        0xbc,0x22,0x00,0x80,0xc7,0x3c,0x88,0x81
};

pub const EFI_MEMORY_ATTRIBUTES_TABLE: Guid = guid_from_raw!{
    0xdcfa911d, 0x26eb, 0x469f,
        0xa2, 0x20, 0x38, 0xb7, 0xdc, 0x46, 0x12, 0x20
};

pub const EFI_CONFORMANCE_PROFILE_TABLE: Guid = guid_from_raw!{
    0x36122546, 0xf7e7, 0x4c8f,
        0xbd, 0x9b, 0xeb, 0x85, 0x25, 0xb5, 0x0c, 0x0b
};

pub const EFI_CONFORMANCE_PROFILES_UEFI_SPEC_GUID: Guid = guid_from_raw!{
    0x523c91af, 0xa195, 0x4382,
        0x81, 0x8d, 0x29, 0x5f, 0xe4, 0x00, 0x64, 0x65
};

pub const EFI_LOADED_IMAGE_PROTOCOL_GUID: Guid = guid_from_raw!{
    0x5B1B31A1,0x9562,0x11d2,
        0x8E,0x3F,0x00,0xA0,0xC9,0x69,0x72,0x3B
};

pub const EFI_SERIAL_IO_PROTOCOL_GUID: Guid = guid_from_raw!{
    0xBB25CF6F,0xF1D4,0x11D2,
        0x9a,0x0c,0x00,0x90,0x27,0x3f,0xc1,0xfd
};

// Memory attributes.
pub const EFI_MEMORY_UC:            u64 = 0x0000000000000001;
pub const EFI_MEMORY_WC:            u64 = 0x0000000000000002;
pub const EFI_MEMORY_WT:            u64 = 0x0000000000000004;
pub const EFI_MEMORY_WB:            u64 = 0x0000000000000008;
pub const EFI_MEMORY_UCE:           u64 = 0x0000000000000010;
pub const EFI_MEMORY_WP:            u64 = 0x0000000000001000;
pub const EFI_MEMORY_RP:            u64 = 0x0000000000002000;
pub const EFI_MEMORY_XP:            u64 = 0x0000000000004000;
pub const EFI_MEMORY_NV:            u64 = 0x0000000000008000;
pub const EFI_MEMORY_MORE_RELIABLE: u64 = 0x0000000000010000;
pub const EFI_MEMORY_RO:            u64 = 0x0000000000020000;
pub const EFI_MEMORY_SP:            u64 = 0x0000000000040000;
pub const EFI_MEMORY_CPU_CRYPTO:    u64 = 0x0000000000080000;
pub const EFI_MEMORY_RUNTIME:       u64 = 0x8000000000000000;
pub const EFI_MEMORY_ISA_VALID:     u64 = 0x4000000000000000;
pub const EFI_MEMORY_ISA_MASK:      u64 = 0x0FFFF00000000000;

/// Description of a mapped region.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct MemoryDescriptor {
    /// Type of the memory region.
    pub memory_type: MemoryType,

    /// Physical address of the first byte of the memory region. This value is
    /// always 4KiB aligned.
    pub physical_start: PhysicalAddress,

    /// Virtual address of the first byte of the memory region. This value is
    /// always 4KiB aligned.
    pub virtual_start: VirtualAddress,

    /// Number of pages in the memory region.
    pub num_pages: u64,

    /// Attributes of the memory region as a bitmask of capabilities for that
    /// memory region. These are not necessarily the current settings of the
    /// memory region.
    pub attribute: u64,
}

impl fmt::Debug for MemoryDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MemoryDescriptor")
            .field("type", &self.memory_type)
            .field("physical start", &format_args!("0x{:016x}", self.physical_start.0))
            .field("virtual start", &format_args!("0x{:016x}", self.virtual_start.0))
            .field("num_pages(4KiB)", &format_args!("{} ({}KiB)", &self.num_pages,
                &self.num_pages * 4))
            .field("attribute", &format_args!("0x{:x}", self.attribute))
            .finish()
    }
}

// TODO
#[repr(C)]
pub struct MemoryAttributesTable {
    version: u32,
    number_of_entries: u32,
    descriptor_size: u32,
    flags: u32,
    entries: FlexibleArray<MemoryDescriptor>,
}

/// A table that allows the platform to advertise its UEFI specification
/// conformance in the form of pre-defined profiles. Each profile is identified
/// by a GUID with known profiles listed somewhere in this document.
///
/// The absense of this table or the presence of this table with
/// `EFI_CONFORMANCE_PROFILES_UEFI_SPEC_GUID` conformance profile GUID shall
/// indicate full conformance to the spec.
///
/// The address reported in the EFI configuration table entry of this type will
/// be referenced as physical and will not be fixed up when transition from
/// preboot to runtime phase.
#[repr(C)]
pub struct ConformanceProfilesTable {
    /// The version of the table. Must be 0x01.
    version: u16,

    /// Number of conformance profile GUID's present in the field
    /// `conformance_profiles`.
    number_of_profiles: u16,

    /// An array of conformance profile GUIDs supported by this system. The C
    /// version of this struct has this field as an incomplete array type.
    conformance_profiles: FlexibleArray<Guid>,
}

impl ConformanceProfilesTable {
    /// Returns a pointer to the beginning of `conformance_profiles` and the
    /// number of profiles present.
    ///
    /// The pointer is created with exposed provenance, as the memory is coming
    /// from the firmware. Preserve the provenance of the returned pointer
    /// while iterating over the `Guid`s, use `map_addr` on the pointer.
    ///
    /// # Safety
    ///
    /// `this` must be a firmware provided pointer to `ConformanceProfilesTable`
    /// whose trailing storage contains `number_of_profiles` contiguous,
    /// properly aligned `Guid` values.
    pub unsafe fn profiles_ptr_and_count(this: *const Self) -> (*const Guid, usize) {
        // Get a pointer to the flexible array.
        let ptr = unsafe { &raw const (*this).conformance_profiles };

        // Create a new pointer with exposed provenance but with the same
        // address as the beginning of the flexible array.
        //
        // Avoid deriving this pointer from `this`'s provenance as `this` points
        // to a struct with known size and flexible array is at the end of that
        // struct and has memory beyond that struct.
        let ptr = core::ptr::with_exposed_provenance(ptr.addr());

        // The number of profiles present.
        let count = unsafe { (*this).number_of_profiles as usize };

        (ptr, count)
    }
}

#[repr(C)]
pub struct SimpleTextInputProtocol {}

/// Minimum interface requirements for text-based console output device.
///
/// Most of these function pointers take a pointer to `Self` which is just a
/// pointer to the `SimpleTextOutputProtocol` instance.
///
/// Read more about these functions in UEFI Spec. Chapter 12, "Protocols -
/// Console Support".
#[repr(C)]
pub struct SimpleTextOutputProtocol {
    /// Reset the console-out device.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.Reset()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-reset)
    pub reset: efiapi!(*mut Self, bool),

    /// Displays the string on the output device at the current cursor position.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.OutputString()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-outputstring)
    pub output_string: efiapi!(*mut Self, *const Char16),

    /// Tests to see whether the console-out device supports this string.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.TestString()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-teststring)
    pub test_string: efiapi!(*mut Self, *const Char16),

    /// Queries information regarding the output devices' supported text modes.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.QueryMode()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-querymode)
    pub query_mode: efiapi!(*mut Self, usize, *mut usize, *mut usize),

    /// Sets the current mode of the output device.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.SetMode()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-setmode)
    pub set_mode: efiapi!(*mut Self, usize),

    /// Sets the foreground and background color of the output text.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.SetAttribute()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-setattribute)
    pub set_attribute: efiapi!(*mut Self, usize),

    /// Clear the screen with the currently set background color.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.ClearScreen()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-clearscreen)
    pub clear_screen: efiapi!(*mut Self),

    /// Sets the current cursor position.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.SetCursorPosition()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-setcursorposition)
    pub set_cursor_position: efiapi!(*mut Self, usize, usize),

    /// Turns the visibility of the cursor on/off.
    ///
    /// See
    /// [EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL.EnableCursor()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-simple-text-output-protocol-enablecursor)
    pub enable_cursor: efiapi!(*mut Self, bool),

    /// Pointer to `SimpleTextOutputMode` data. This is read-only. Use the
    /// appropriate functions in this interface to change mode values.
    pub mode: *const SimpleTextOutputMode,
}

/// Information about the current output mode.
///
/// The following data values are read only and are changed with the appropriate
/// interface functions in `SimpleTextOutputProtocol`.
///
/// Read more about this struct in UEFI Spec. Chapter 12, "Protocols - Console
/// Support".
#[repr(C)]
pub struct SimpleTextOutputMode {
    /// Number of modes supported by `set_mode()` and `query_mode()`.
    max_mode: u32,

    /// The text mode of the output device(s),
    mode: u32,

    /// The current character output attribute.
    attribute: u32,

    /// The cursors' column.
    cursor_column: u32,

    /// The cursor's row.
    cursor_row: u32,

    /// Whether the cursor is currently visible or not.
    cursor_visible: bool,
}

#[derive(Debug)]
#[repr(C)]
pub struct SerialIoProtocol {
    pub revision: u32,

    /// See
    /// [EFI_SERIAL_IO_PROTOCOL.Reset()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-serial-io-protocol-reset)
    pub reset: efiapi!(*mut Self),

    /// See
    /// [EFI_SERIAL_IO_PROTOCOL.SetAttribute()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-serial-io-protocol-setattribute)
    pub set_attribute: efiapi!(*mut Self, u64, u32, u32, ParityType, u8, StopBitsType),

    /// See
    /// [EFI_SERIAL_IO_PROTOCOL.SetControl()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-serial-io-protocol-setcontrol)
    pub set_control: efiapi!(*mut Self, u32),

    /// See
    /// [EFI_SERIAL_IO_PROTOCOL.GetControl()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-serial-io-protocol-getcontrol)
    pub get_control: efiapi!(*mut Self, *mut u32),

    /// See
    /// [EFI_SERIAL_IO_PROTOCOL.Write()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-serial-io-protocol-write)
    pub write: efiapi!(*mut Self, *mut usize, *const c_void),

    /// See
    /// [EFI_SERIAL_IO_PROTOCOL.Read()](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-serial-io-protocol-read)
    pub read: efiapi!(*mut Self, *mut usize, *mut c_void),

    pub mode: *mut SerialIoMode,

    pub device_type_guid: *const Guid,
}

/// Parity types for Serial IO protocol.
///
/// Valid values are variants of the enum `EfiParityType`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ParityType(u32);

impl From<EfiParityType> for ParityType {
    /// All `EfiParityType` values are valid `ParityType` values.
    fn from(val: EfiParityType) -> Self {
        Self(val as u32)
    }
}

impl fmt::Debug for ParityType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // If parity type is a valid EfiParityType variant then print it as such, if
        // not then do the normal printing.

        if let Ok(efi_parity) = EfiParityType::try_from(*self) {
            write!(f, "{:?}", efi_parity)
        } else {
            f.debug_tuple("ParityType")
                .field(&self.0)
                .finish()
        }
    }
}

/// Parity values defined by the main UEFI spec. for Serial IO protocol.
///
/// This must not be used directly in the EFIAPI functions, instead use
/// `ParityType` that is build using this type.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum EfiParityType {
    DefaultParity = 0,
    NoParity,
    Even,
    Odd,
    Mark,
    Space,
}

impl TryFrom<ParityType> for EfiParityType {
    type Error = ();

    /// Some `ParityType` values are valid `EfiParityType` values.
    fn try_from(val: ParityType) -> Result<Self, Self::Error> {
        let default_parity = EfiParityType::DefaultParity as u32;
        let space_parity = EfiParityType::Space as u32;

        let r1 = RangeInclusive::new(default_parity, space_parity);

        if r1.contains(&val.0) {
            let efi_parity = unsafe { core::mem::transmute(val.0) };
            Ok(efi_parity)
        } else {
            Err(())
        }
    }
}

#[repr(u32)]
pub enum StopBitsType {
    DefaultStopBits = 0,
    OneStopBit,
    OneFiveStopBits,
    TwoStopBits,
}

#[repr(C)]
pub struct SerialIoMode {
    control_mask: u32,
    timeout: u32,
    baudrate: u64,
    received_fifo_depth: u32,
    data_bits: u32,
    parity: u32,
    stop_bits: u32,
}

impl fmt::Debug for SerialIoMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SerialIoMode")
            .field("control_mask", &format_args!("0x{:x}", self.control_mask))
            .field("timeout", &format_args!("{} us", self.timeout))
            .field("baudrate", &self.baudrate)
            .field("received_fifo_depth", &self.received_fifo_depth)
            .field("data_bits", &self.data_bits)
            .field("parity", &format_args!("{} (check ParityType)", &self.parity))
            .field("stop_bits", &format_args!("{} (check StopBitsType)", &self.stop_bits))
            .finish()
    }
}

#[repr(u32)]
pub enum LocateSearchType {
    AllHandles = 0,
    ByRegisterNotify,
    ByProtocol,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ResetType(u32);

impl From<EfiResetType> for ResetType {
    fn from(val: EfiResetType) -> Self {
        Self(val as u32)
    }
}

impl fmt::Debug for ResetType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(efi_reset) = EfiResetType::try_from(*self) {
            write!(f, "{:?}", efi_reset)
        } else {
            write!(f, "ResetType({})", self.0)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum EfiResetType {
    Cold = 0,
    Warm,
    Shutdown,
    PlatformSpecific,
}

impl TryFrom<ResetType> for EfiResetType {
    type Error = ();

    fn try_from(val: ResetType) -> Result<Self, Self::Error> {
        let reset_cold = EfiResetType::Cold as u32;
        let reset_platform_specific = EfiResetType::PlatformSpecific as u32;

        let r = RangeInclusive::new(reset_cold, reset_platform_specific);

        if r.contains(&val.0) {
            let efi_reset = unsafe { core::mem::transmute(val.0) };
            Ok(efi_reset)
        } else {
            Err(())
        }
    }
}
