use core::ffi::c_void;
// use core::fmt;
// use core::ops::RangeInclusive;

use crate::uefi::*;
use crate::guid_from_raw;

pub const EFI_LOADED_IMAGE_PROTOCOL_REVISION: u32 = 0x1000;

pub const EFI_LOADED_IMAGE_PROTOCOL_GUID: Guid = guid_from_raw!{
    0x5B1B31A1,0x9562,0x11d2,
        0x8E,0x3F,0x00,0xA0,0xC9,0x69,0x72,0x3B
};

/// Can be used on any image handle to obtain information about loaded image.
///
/// Each loaded image has an image handle that supports this protocol. When the
/// image is started, it is passed an image handle for itself that can be used
/// to access this protocol.
#[repr(C)]
pub struct LoadedImageProtocol {
    /// Defines the revision number for this protocol structure. Currently this
    /// value should be [`EFI_LOADED_IMAGE_PROTOCOL_REVISION`].
    pub revision: u32,

    /// Parent image's image handle. It is null if the image is loaded directly
    /// from the firmware's boot manager.
    pub parent_handle: Handle,

    /// The EFI images' system table pointer.
    pub system_table: *mut SystemTable,

    /// The device handle that the EFI image was loaded from.
    pub device_handle: Handle, 

    /// A pointer to the file path portion specific to
    /// [`device_handle`](Self::device_handle) that the EFI image was loaded
    /// from.
    pub file_path: *mut DevicePathProtocol,

    /// **DO NOT USE**.
    pub __reserved: *const c_void,

    /// The size in bytes of [`load_options`](Self::load_options).
    pub load_options_size: u32,

    /// A pointer to the images' binary load options. See "Load Option" section
    /// in "Boot Manager" chapter of the main UEFI spec.
    pub load_options: *mut c_void,

    /// Base address at which the image was loaded.
    pub image_base: *mut c_void,

    /// The size in bytes of the loaded image.
    pub image_size: u64,
    
    /// The memory type with which the code sections were loaded.
    pub image_code_type: MemoryType,

    /// The memory type with which the data sections were loaded.
    pub image_data_type: MemoryType,

    /// Function that unloads the image.
    ///
    /// See
    /// [EFI_LOADED_IMAGE_PROTOCOL.Unload()](https://uefi.org/specs/UEFI/2.10/09_Protocols_EFI_Loaded_Image.html#efi-loaded-image-protocol-unload)
    pub unload: efiapi!(Handle),
}

pub const EFI_LOADED_IMAGE_DEVICE_PATH_PROTOCOL_GUID: Guid = guid_from_raw!{
    0xbc62157e,0x3e33,0x4fec,
        0x99,0x20,0x2d,0x3b,0x36,0xd7,0x50,0xdf
};

/// Describes the device path that was used when the PE/COFF image was loaded.
///
/// It has exactly the same structure as [`DevicePathProtocol`], except it has a
/// different GUID.
pub type LoadedImageDevicePathProtocol = DevicePathProtocol;

pub const EFI_DEVICE_PATH_PROTOCOL_GUID: Guid = guid_from_raw!{
    0x09576e91,0x6d3f,0x11d2,
        0x8e,0x39,0x00,0xa0,0xc9,0x69,0x72,0x3b
};

#[repr(C)]
pub struct DevicePathProtocol {
    dev_path_type: u8,
    sub_type: u8,
    length: [u8; 2],
    data: FlexibleArray<u8>,
}

pub const EFI_DEVICE_PATH_TO_TEXT_PROTOCOL_GUID: Guid = guid_from_raw!{
    0x8b843e20,0x8132,0x4852,
        0x90,0xcc,0x55,0x1a,0x4e,0x4a,0x7f,0x1c
};

/// Common functions to convert device node and path to textual representations.
#[repr(C)]
pub struct DevicePathToTextProtocol {
    /// Converts device node to its textual representation.
    ///
    /// See
    /// [EFI_DEVICE_PATH_TO_TEXT_PROTOCOL.ConvertDeviceNodeToText()](https://uefi.org/specs/UEFI/2.10/10_Protocols_Device_Path_Protocol.html#efi-device-path-to-text-protocol-convertdevicenodetotext)
    pub device_node_to_text: unsafe extern "efiapi" fn(*const DevicePathProtocol,
        bool, bool) -> *const Char16,

    /// Converts device path to its textual representation.
    ///
    /// See
    /// [EFI_DEVICE_PATH_TO_TEXT_PROTOCOL.ConvertDevicePathToText()](https://uefi.org/specs/UEFI/2.10/10_Protocols_Device_Path_Protocol.html#efi-device-path-to-text-protocol-convertdevicepathtotext)
    pub device_path_to_text: unsafe extern "efiapi" fn(*const DevicePathProtocol,
        bool, bool) -> *const Char16,
}

/// The version of the [`FileProtocol`] specified by UEFI spec., all future
/// versions will be backwards compatible. If they are not then they will have
/// different GUID for `SimpleFileSysteProtocol`.
///
/// Is it the version of `FileProtocol` or `SimpleFileSystemProtocol`...?
pub const EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_REVISION: u64 = 0x00010000;

// Revision from `SimpleFileSystemProtocol` came 0x10000 but `FileProtocol`
// revision came to be 0x20000.

/// Guid for [`SimpleFileSystemProtocol`] protocol.
pub const EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: Guid = guid_from_raw!{
    0x0964e5b22,0x6459,0x11d2,
        0x8e,0x39,0x00,0xa0,0xc9,0x69,0x72,0x3b
};

#[repr(C)]
pub struct SimpleFileSystemProtocol {
    /// The version of the EFI_FILE_PROTOCOL, or [`FileProtocol`]. The version
    /// specified by 2.10 spec. is 
    pub revision: u64,

    /// Opens the volume for file IO access.
    ///
    /// See
    /// [EFI_SIMPLE_FILE_SYSTEM_PROTOCOL.OpenVolume()](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html#efi-simple-file-system-protocol-openvolume)
    pub open_volume: efiapi!(*mut Self, *mut *mut FileProtocol),
}

pub const EFI_FILE_PROTOCOL_REVISION:  u64 = 0x00010000; // revision 1.0
pub const EFI_FILE_PROTOCOL_REVISION2: u64 = 0x00020000; // Latest as per v2.10

/// Interface to access EFI-supported file systems.
#[repr(C)]
pub struct FileProtocol {
    /// The version of the `FileProtocol` interface. All future version are
    /// backward compatible with the revision 1.0. Revision 2 added the `*_ex`
    /// functions at the end of this interface.
    pub revision: u64,

    /// Opens or creates a new file.
    ///
    /// See
    /// [EFI_FILE_PROTOCOL.Open()](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html#efi-file-protocol-open)
    pub open: efiapi!(*mut Self, *mut *mut FileProtocol, *const Char16, u64, u64),

    /// Closes a specified file handle.
    ///
    /// See
    /// [EFI_FILE_PROTOCOL.Close()](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html#efi-file-protocol-close)
    pub close: efiapi!(*mut Self),

    _delete: efiapi!(),

    /// Reads data from a file.
    ///
    /// See
    /// [EFI_FILE_PROTOCOL.Read()](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html#efi-file-protocol-read)
    pub read: efiapi!(*mut Self, *mut usize, *mut c_void),

    _write: efiapi!(),

    _get_position: efiapi!(),

    _set_position: efiapi!(),

    /// Returns information about a file. It takes GUID of the kind of
    /// information. Some relevent info structs are [`FileSystemInfo`] and
    /// [`FileInfo`].
    ///
    /// See
    /// [EFI_FILE_PROTOCOL.GetInfo()](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html#efi-file-protocol-getinfo)
    pub get_info: efiapi!(*mut Self, *const Guid, *mut usize, *mut c_void),

    _set_info: efiapi!(),

    _flush: efiapi!(),

    // These fields were introduced in the revision 2.0. Not absolutely
    // necessary, so ignore them. All future revisions of this struct are
    // guaranteed to be backward compatible with revision 1.0. So we work with
    // the v1.0 fields.

    // _open_ex: efiapi!(),
    // _read_ex: efiapi!(),
    // _write_ex: efiapi!(),
    // _flush_ex: efiapi!(),
}

/// GUID to use with [`FileProtocol::get_info`] to get generic file
/// information.
pub const EFI_FILE_INFO_GUID: Guid = guid_from_raw!{
    0x09576e92,0x6d3f,0x11d2,
        0x8e,0x39,0x00,0xa0,0xc9,0x69,0x72,0x3b
};

/// Data structure that contains generic file information.
#[repr(C)]
pub struct FileInfo {
    pub info_size: u64,
    pub file_size: u64,
    pub physical_size: u64,
    pub create_time: Time,
    pub last_access_time: Time,
    pub modification_time: Time,
    pub attribute: u64,
    pub filename: FlexibleArray<Char16>,
}

impl FileInfo {
    /// Debug dump the `FileInfo` pointed to by `info` on serial output.
    ///
    /// # Safety
    ///
    /// The info pointer must be a firmware provided location. The `FileInfo`
    /// struct has an incomplete array in the end, which can not be created on
    /// stack, so use the firmware provided pointer here, instead of reading it
    /// on stack and providin a reference to it.
    pub unsafe fn dump_to_serial(info: *const FileInfo) {
        use crate::{serialn, serial};

        unsafe {
            serialn!("info_size:     {} bytes", (*info).info_size);
            serialn!("file_size:     {} bytes", (*info).file_size);
            serialn!("physical_size: {} bytes", (*info).physical_size);

            let c_t = (*info).create_time;
            serialn!("create_time:   {}-{}-{}, {}:{}:{}", c_t.day, c_t.month,
                c_t.year, c_t.hour, c_t.minute, c_t.second);

            let la_t = (*info).last_access_time;
            serialn!("last access:   {}-{}-{}, {}:{}:{}", la_t.day, la_t.month,
                la_t.year, la_t.hour, la_t.minute, la_t.second);

            let m_t = (*info).modification_time;
            serialn!("modification:  {}-{}-{}, {}:{}:{}", m_t.day, m_t.month,
                m_t.year, m_t.hour, m_t.minute, m_t.second);

            serialn!("attribute:     0x{:x}", (*info).attribute);

            serial!("file name:     ");
            crate::serial::serialn_write_raw((&raw const (*info).filename).cast());
        }
    }
}

/// GUID to use with [`FileProtocol::get_info`] to get system volume
/// information.
pub const EFI_FILE_SYSTEM_INFO_ID: Guid = guid_from_raw!{
    0x09576e93,0x6d3f,0x11d2,
        0x8e,0x39,0x00,0xa0,0xc9,0x69,0x72,0x3b
};

/// Data structure that contains system volume information.
#[repr(C)]
pub struct FileSystemInfo {
    pub info_size: u64,
    pub read_only: bool,
    pub volume_size: u64,
    pub free_space: u64,
    pub block_size: u64,
    pub volume_label: FlexibleArray<Char16>,
}

pub const EFI_ACPI_20_TABLE_GUID: Guid = guid_from_raw!{
    0x8868e871,0xe4f1,0x11d3,
        0xbc,0x22,0x00,0x80,0xc7,0x3c,0x88,0x81
};
