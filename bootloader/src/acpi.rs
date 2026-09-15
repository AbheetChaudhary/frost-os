use core::ptr::NonNull;

use crate::uefi::FlexibleArray;
use crate::mm::{PhysAddr32, PhysAddr64};

/// Expected signature for RSDP table. It is 8 bytes instead of the regular 4.
pub const SIG_RSDP: [u8; 8] = *b"RSD PTR ";

/// Expected signature for RSDT table.
pub const SIG_RSDT: [u8; 4] = *b"RSDT";

/// Expected signature for XSDT table.
pub const SIG_XSDT: [u8; 4] = *b"XSDT";

/// Expected signature for FADT table.
pub const SIG_FADT: [u8; 4] = *b"FACP";

/// Extended signature for Differentiated System Description Table.
pub const SIG_DSDT: [u8; 4] = *b"DSDT";

/// Calculate the wrapping sum of a slice of bytes.
///
/// Useful for calculating checksums.
fn fold_wrapping_sum(s: &[u8]) -> u8 {
    s.iter().fold(0, |acc, b| acc.wrapping_add(*b))
}

/// Error type related to RSDP.
///
/// It contains variants that are generic ACPI error like those related to
/// invalid signature, checksum, etc., along with some RSDP specific variants.
#[derive(Debug, Copy, Clone)]
pub enum RsdpError {
    InvalidSignature,
    InvalidChecksum,

    /// Revision number is not 0 or 2. Only these two revisions are supported.
    InvalidRevision,
}

/// Raw Root System Description Pointer Structure for ACPI 1.0 and onwards.
///
/// This struct might be embedded in [`RawRsdp2`]. Use the `revision` field to
/// check that.
///
/// This is a raw version of this struct, read directly from memory. Its fields
/// must be properly validated for correctness, beyond checksum and signature,
/// before it could be freely used in safe code.
///
#[repr(C, packed)]
pub struct RawRsdp1 {
    /// Table signature: "RSD PTR ".
    signature: [u8; 8],

    /// Checksum of the fields defined in ACPI 1.0 spec. Byte 0-19, including
    /// this checksum field must sum to zero.
    checksum: u8,

    /// OEM supplied string to identify OEM.
    oem_id: [u8; 6],

    /// The revision of this structure. ACPI version 1.0 revision number is 0.
    /// Later revisions use 2, last checked for ACPI 6.4.
    /// 
    /// If the revision number is 2 then it is part of a [`RawRsdp2`] structure.
    revision: u8,

    /// 32-bit physical address of RSDT table.
    pub rsdt_addr: PhysAddr32,
}

impl RawRsdp1 {
    /// Tries to read a `RawRsdp1` structure from a pointer.
    ///
    /// It might fail due to invalid signature or checksum, returning `Err` in
    /// those cases. Other fields must be seperately evaluated for correctness
    /// before freely using it in safe code.
    ///
    /// # Safety
    ///
    /// The pointer must be valid for read of `size_of::<RawRsdp1>()` bytes.
    pub unsafe fn read_from_ptr(ptr: *const RawRsdp1) -> Result<RawRsdp1, RsdpError> {
        // Read table signature.
        let signature: [u8; 8] = unsafe {
            ptr.cast::<[u8; 8]>().read()
        };

        // Return Err in case of invalid signature.
        if signature != SIG_RSDP {
            return Err(RsdpError::InvalidSignature);
        }

        // Checksum of the struct. It is just a u8 so alignment does not matter.
        let checksum = unsafe {
            (&raw const (*ptr).checksum).read()
        };

        // OEM id is an array of 6 u8's so it is well aligned.
        let oem_id = unsafe {
            (&raw const (*ptr).oem_id).read()
        };

        // revision is just a u8 so it is well aligned.
        let revision = unsafe {
            (&raw const (*ptr).revision).read()
        };

        // Revision should be 0 for ACPI 1.0 or 2 for later versions.
        if revision != 0 && revision != 2 {
            return Err(RsdpError::InvalidRevision);
        }

        // u32 may not be properly aligned.
        let rsdt_addr = unsafe {
            (&raw const (*ptr).rsdt_addr).read_unaligned()
        };

        // Calculate checksum as the sum of all the bytes.
        let sum = checksum
            .wrapping_add(fold_wrapping_sum(signature.as_slice()))
            .wrapping_add(fold_wrapping_sum(oem_id.as_slice()))
            .wrapping_add(revision)
            .wrapping_add(fold_wrapping_sum(&rsdt_addr.as_u32().to_ne_bytes()));

        // If bytes did not sum up to zero then invalid checksum.
        if sum != 0 {
            return Err(RsdpError::InvalidChecksum);
        }

        let rsdp = RawRsdp1 {
            signature,
            checksum,
            oem_id,
            revision,
            rsdt_addr,
        };

        Ok(rsdp)
    }

    pub fn revision(&self) -> u8 {
        self.revision
    }

    pub fn rsdt_addr(&self) -> PhysAddr32 {
        self.rsdt_addr
    }
}

/// Raw Root System Description Pointer from ACPI 2.0 onwards.
///
/// This is a raw version of this struct, read directly from memory. Its fields
/// must be properly validated for correctness, beyond checksum and signature,
/// before it could be freely used in safe code.
///
/// Last checked with ACPI 6.4a.
///
#[repr(C, packed)]
pub struct RawRsdp2 {
    /// `RawRsdp2` structure is backwards compatible with [`RawRsdp1`], which is just
    /// extended by adding a few extra fields in the end to get to `RawRsdp2`.
    ///
    /// Which one is it is determined by the [`RawRsdp1::revision`] field which
    /// must be 2 for `RawRsdp2`.
    rsdp_rev1: RawRsdp1,

    /// The length of the entire table.
    length: u32,

    /// 64-bit physical address of XSDT.
    xsdt_addr: PhysAddr64,

    /// Checksum of the entire table, including both checksum fields.
    extended_checksum: u8,

    /// Reserved field.
    _reserved: [u8; 3],
}

impl RawRsdp2 {
    /// Tries to read a `RawRsdp2` structure from a pointer.
    ///
    /// It might fail due to invalid signature or checksum, returning `Err` in
    /// those cases. Other fields must be seperately evaluated for correctness
    /// before freely using it in safe code.
    ///
    /// # Safety
    ///
    /// The pointer must be valid for read of `size_of::<RawRsdp2>()` bytes.
    pub unsafe fn read_from_ptr(ptr: *const RawRsdp2) -> Result<RawRsdp2, RsdpError> {
        // Parse the initial bytes as RawRsdp1, if that fails then early return.
        let rsdp1 = unsafe {
            RawRsdp1::read_from_ptr(ptr.cast())?
        };

        // This field requires unaligned read.
        let length = unsafe {
            (&raw const (*ptr).length).read_unaligned() 
        };

        // This field requires unaligned read.
        let xsdt_addr = unsafe {
            (&raw const (*ptr).xsdt_addr).read_unaligned() 
        };

        // This is just a u8 so it could be read normally.
        let ex_checksum = unsafe {
            (&raw const (*ptr).extended_checksum).read()
        };

        // Since rsdp_rev1 already has correct checksum, otherwise we wouldn't
        // be on this line, so all the fields of rsdp_rev1 already sum to 0. We
        // just need to make sure that the remaining fields also sum to zero.
        let sum = ex_checksum
            .wrapping_add(fold_wrapping_sum(&length.to_ne_bytes()))
            .wrapping_add(fold_wrapping_sum(&xsdt_addr.as_u64().to_ne_bytes()));

        if sum != 0 {
            return Err(RsdpError::InvalidChecksum);
        }

        let rsdp2 = RawRsdp2 {
            rsdp_rev1: rsdp1,
            length,
            xsdt_addr,
            extended_checksum: ex_checksum,
            _reserved: [0u8; 3],
        };

        Ok(rsdp2)
    }

    pub fn revision(&self) -> u8 {
        self.rsdp_rev1.revision()
    }

    pub fn rsdt_addr(&self) -> PhysAddr32 {
        self.rsdp_rev1.rsdt_addr()
    }

    pub fn xsdt_addr(&self) -> PhysAddr64 {
        self.xsdt_addr
    }
}

#[repr(C, packed)]
pub struct Gas {
    address_space_id: u8,
    register_bit_width: u8,
    register_bit_offset: u8,
    access_size: u8,
    address: u64,
}

/// System Description Table Header.
///
/// Last checked with ACPI 6.4a.
#[repr(C, packed)]
pub struct SystemDescTableHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

/// Raw Root System Description Table.
///
/// This struct has an incomplete array as its last field so it must be used
/// only with pointers. It must not be created on the stack as an owned value.
///
/// This is a raw version of this struct, read directly from memory. Its fields
/// must be properly validated for correctness, beyond checksum and signature,
/// before it could be freely used in safe code.
///
/// Last checked with ACPI 6.4a.
///
#[repr(C, packed)]
pub struct RawRsdt {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
    entry: FlexibleArray<PhysAddr32>,
}

#[derive(Debug, Copy, Clone)]
pub enum RsdtError {
    InvalidSignature,
    InvalidChecksum,
    InvalidRevision,
    InvalidLength,
    NullPtrRawRsdt,
}

#[derive(Debug, Copy, Clone)]
pub enum XsdtError {
    InvalidSignature,
    InvalidChecksum,
    InvalidRevision,
    InvalidLength,
    NullPtrRawXsdt,
}

#[repr(C, packed)]
pub struct RawXsdt {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
    entry: FlexibleArray<PhysAddr64>,
}

pub struct XsdtRef(NonNull<RawXsdt>);

impl XsdtRef {
    pub unsafe fn from_raw_xsdt_ptr(ptr: *const RawXsdt) -> Result<XsdtRef, XsdtError> {
        if ptr.is_null() {
            return Err(XsdtError::NullPtrRawXsdt);
        }

        let signature: [u8; 4] = unsafe {
            ptr.cast::<[u8; 4]>().read()
        };

        if signature != SIG_XSDT {
            return Err(XsdtError::InvalidSignature);
        }

        let length = unsafe {
            (&raw const (*ptr).length).read_unaligned()
        };

        let revision = unsafe {
            (&raw const (*ptr).revision).read()
        };

        if revision != 1 {
            return Err(XsdtError::InvalidRevision);
        }

        let checksum = unsafe {
            (&raw const (*ptr).checksum).read()
        };

        let oem_id = unsafe {
            (&raw const (*ptr).oem_id).read()
        };

        let oem_table_id = unsafe {
            (&raw const (*ptr).oem_table_id).read()
        };

        let oem_revision = unsafe {
            (&raw const (*ptr).oem_revision).read_unaligned()
        };

        let creator_id = unsafe {
            (&raw const (*ptr).creator_id).read_unaligned()
        };

        let creator_revision = unsafe {
            (&raw const (*ptr).creator_revision).read_unaligned()
        };

        // Make sure that there are 8*n bytes at the end from entry onwards.

        // The bytes we must skip before reaching the 64-bit address array at
        // the struct end.
        let skip_bytes = size_of::<RawXsdt>();

        // Number of bytes making up the entry array.
        let array_bytes = length as usize - skip_bytes;

        // Validate that the remaining bytes are a multiple of 8.
        if array_bytes % 8 != 0 {
            return Err(XsdtError::InvalidLength);
        }

        let entry_count = array_bytes / 8;

        // Forge a pointer with exposed provenance that we can use to access
        // members of the entry array.
        let entry_addr = core::hint::black_box(unsafe { 
            (&raw const (*ptr).entry).addr()
        });

        let entry_ptr: *const u64 = core::ptr::with_exposed_provenance(entry_addr);

        // Calculate wrapping sum of all the bytes of the entry array.
        let mut entry_sum = 0u8;
        for i in 0..entry_count {
            let e = unsafe { entry_ptr.add(i).read_unaligned() };
            entry_sum += fold_wrapping_sum(&e.to_ne_bytes());
        }

        let final_sum = entry_sum
            .wrapping_add(fold_wrapping_sum(&signature))
            .wrapping_add(fold_wrapping_sum(&length.to_ne_bytes()))
            .wrapping_add(revision)
            .wrapping_add(checksum)
            .wrapping_add(fold_wrapping_sum(&oem_id))
            .wrapping_add(fold_wrapping_sum(&oem_table_id))
            .wrapping_add(fold_wrapping_sum(&oem_revision.to_ne_bytes()))
            .wrapping_add(fold_wrapping_sum(&creator_id.to_ne_bytes()))
            .wrapping_add(fold_wrapping_sum(&creator_revision.to_ne_bytes()));

        if final_sum != 0 {
            return Err(XsdtError::InvalidChecksum);
        }

        let ptr = NonNull::new(ptr.cast_mut()).ok_or(XsdtError::NullPtrRawXsdt)?;

        Ok(Self(ptr))
    }

    fn length(&self) -> usize {
        let ptr = self.0.as_ptr();

        let length = unsafe {
            (&raw const (*ptr).length).read_unaligned()
        };

        length as usize
    }

    fn get_entry_ptr(&self) -> *const u64 {
        let ptr = self.0.as_ptr();

        let entry_addr = unsafe {
            (&raw const (*ptr).entry).addr()
        };

        // Why tho?
        //
        // By accessing beyond entry we are going to access memory outside of
        // this struct, outside in the sense that memory beyond any field of the
        // struct we have described.
        //
        // This next line is to pretend that there is some allocation at the end
        // of entry field that we know about through some means that is outside
        // the rust AM, and we are creating a pointer that is valid for
        // accessing that memory.
        let mystery_addr = core::hint::black_box(entry_addr);
        core::ptr::with_exposed_provenance(mystery_addr)
    }

    pub fn find_sdth_addr(&self, sig: [u8; 4]) -> Option<u64> {
        let skip_bytes = size_of::<RawXsdt>();

        let sdth_count = (self.length() - skip_bytes) / 8;

        let sdth_ptr_begin = self.get_entry_ptr();

        for i in 0..sdth_count {
            let ptr = unsafe { sdth_ptr_begin.add(i) };

            let sdth_addr = unsafe { ptr.read_unaligned() };

            let sdth_ptr: *const SystemDescTableHeader =
                core::ptr::with_exposed_provenance(sdth_addr.try_into().unwrap());

            let signature = unsafe {
                (&raw const (*sdth_ptr).signature).read()
            };

            if signature == sig {
                return Some(sdth_addr);
            }
        }

        None
    }

    pub fn dump_header_signatures(&self) {
        let skip_bytes = size_of::<RawXsdt>();

        let sdth_count = (self.length() - skip_bytes) / 8;

        let sdth_ptr_begin = self.get_entry_ptr();

        crate::serial!("dumping {} signatures.\n", sdth_count);

        for i in 0..sdth_count {
            let ptr = unsafe { sdth_ptr_begin.add(i) };

            let sdth_addr = unsafe { ptr.read_unaligned() };

            let sdth_ptr: *const SystemDescTableHeader =
                core::ptr::with_exposed_provenance(sdth_addr.try_into().unwrap());

            let signature = unsafe {
                (&raw const (*sdth_ptr).signature).read()
            };

            let sig_bytes = str::from_utf8(&signature).unwrap();
            crate::serial!("{}\n", sig_bytes);
        }
    }
}
