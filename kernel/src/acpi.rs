use core::mem::offset_of;

macro_rules! as_cast {
    ($e:expr, $src_type:ty => $dst_type:ty) => {{
        let __t: $src_type = From::from($e);
        __t as $dst_type
    }};
}

use crate::{address, serial, page_tables, cpu, utils, isr, gdt};

use address::{PhysAddr32, PhysAddr64, PhysAddr};

#[repr(transparent)]
pub struct FlexibleArray<T>(core::marker::PhantomData<T>);

/// Expected signature for RSDP table. It is 8 bytes instead of the regular 4.
pub const SIG_RSDP: [u8; 8] = *b"RSD PTR ";

/// Expected signature for RSDT table.
pub const SIG_RSDT: [u8; 4] = *b"RSDT";

/// Expected signature for XSDT table.
pub const SIG_XSDT: [u8; 4] = *b"XSDT";

/// Expected signature for FADT table.
pub const SIG_FADT: [u8; 4] = *b"FACP";

/// Expected signature for MADT table.
pub const SIG_MADT: [u8; 4] = *b"APIC";

/// Extended signature for Differentiated System Description Table.
pub const SIG_DSDT: [u8; 4] = *b"DSDT";

/// Calculate the wrapping sum of a slice of bytes.
///
/// Useful for calculating checksums.
fn fold_wrapping_sum(s: &[u8]) -> u8 {
    s.iter().fold(0, |acc, b| acc.wrapping_add(*b))
}

unsafe fn wrapping_sum_raw_slice(ptr: *const u8, len: usize) -> u8 {
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };

    s.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// System Description Table Header.
///
/// Last checked with ACPI 6.4a.
#[repr(C, packed)]
pub struct RawTableHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

impl RawTableHeader {
    pub fn unpack(self) -> TableHeader {
        let RawTableHeader {
            signature, length, revision, checksum, oem_id,
            oem_table_id, oem_revision, creator_id, creator_revision } = self;

        TableHeader {
            signature, length, revision, checksum, oem_id,
            oem_table_id, oem_revision, creator_id, creator_revision,
        }
    }

    pub fn calculate_checksum(&self) -> u8 {
        let ptr: *const u8 = core::ptr::from_ref(self).cast();

        unsafe {
            wrapping_sum_raw_slice(ptr, size_of::<RawTableHeader>())
        }
    }
}

pub struct TableHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

#[derive(Debug, Copy, Clone)]
pub enum AcpiError {
    InvalidSignature,
    InvalidChecksum,
    InvalidRevision { expected: u8, got: u8 },
    CorruptedFields,
}

/// Extended Raw Root System Description Table.
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
pub struct RawXsdt {
    header: RawTableHeader,
    entry: FlexibleArray<PhysAddr64>,
}

pub struct Xsdt {
    header: TableHeader,
    entry_ptr: *const PhysAddr64,
    entry_count: usize,
}

impl Xsdt {
    pub unsafe fn from_raw_xsdt_ptr(ptr: *const RawXsdt) -> Result<Xsdt, AcpiError> {
        let raw_header: RawTableHeader;

        unsafe {
            let signature: [u8; 4] = (&raw const (*ptr).header.signature).read();

            if signature != SIG_XSDT {
                return Err(AcpiError::InvalidSignature);
            }

            raw_header = (&raw const (*ptr).header).read_unaligned();
        }

        let header_sum = raw_header.calculate_checksum();

        let header = raw_header.unpack();

        if header.revision != 1 {
            return Err(AcpiError::InvalidRevision {
                expected: 1,
                got: header.revision,
            });
        }

        // Make sure that there are 8*n bytes at the end from entry onwards.

        // The bytes we must skip before reaching the 64-bit address array at
        // the struct end.
        let skip_bytes = size_of::<RawXsdt>();

        // Number of bytes making up the entry array.
        let entry_arr_size = header.length as usize - skip_bytes;

        // Validate that the remaining bytes are a multiple of 8.
        if entry_arr_size % 8 != 0 {
            return Err(AcpiError::CorruptedFields);
        }

        let entry_count = entry_arr_size / 8;

        let entry_ptr: *const u64 = unsafe {
            (&raw const (*ptr).entry).cast()
        };

        // Calculate wrapping sum of all the bytes of the entry array.
        let mut entry_sum = 0u8;
        for i in 0..entry_count {
            let e = unsafe { entry_ptr.add(i).read_unaligned() };
            entry_sum += fold_wrapping_sum(&e.to_ne_bytes());
        }

        let final_sum = entry_sum.wrapping_add(header_sum);

        if final_sum != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        let xsdt = Xsdt {
            header,
            entry_ptr: entry_ptr.cast(),
            entry_count,
        };

        Ok(xsdt)
    }

    pub fn find_sdth_addr(&self, sig: [u8; 4]) -> Option<u64> {
        todo!();
        /*
        let skip_bytes = size_of::<RawXsdt>();

        let sdth_count = (self.length() - skip_bytes) / 8;

        let sdth_ptr_begin = self.get_entry_ptr();

        for i in 0..sdth_count {
            let ptr = unsafe { sdth_ptr_begin.add(i) };

            let sdth_addr = unsafe { ptr.read_unaligned() };

            let sdth_ptr: *const TableHeader =
                core::ptr::with_exposed_provenance(sdth_addr.try_into().unwrap());

            let signature = unsafe {
                (&raw const (*sdth_ptr).signature).read()
            };

            if signature == sig {
                return Some(sdth_addr);
            }
        }

        None
        */
    }
}

#[repr(C, packed)]
pub struct RawMadt {
    header: RawTableHeader,

    local_int_controller_addr: PhysAddr32,

    flags: u32,

    interrupt_controller_structs: FlexibleArray<u8>,
}

pub struct Madt {
    header: TableHeader,

    /// CPU local interrupt APIC controller physical address.
    local_apic_addr: PhysAddr32,

    /// Has dual-8259 PIC support.
    dual8259: bool,

    /// Interrupt Controller Structure beginning.
    ics_begin: *const u8,

    /// Total bytes for all ICS.
    ics_len: usize,
}

impl Madt {
    pub unsafe fn from_raw_madt_ptr(ptr: *const RawMadt) -> Result<Madt, AcpiError> {
        let raw_header: RawTableHeader;

        unsafe {
            let header_ptr = &raw const (*ptr).header;

            let sig = (&raw const (*header_ptr).signature).read();
            if sig != SIG_MADT {
                return Err(AcpiError::InvalidSignature);
            }

            raw_header = header_ptr.read_unaligned();
        }

        let header_sum = raw_header.calculate_checksum();

        let header = raw_header.unpack();

        if header.revision != 3 {
            return Err(AcpiError::InvalidRevision {
                expected: 3,
                got: header.revision,
            });
        }

        let skip_bytes = offset_of!(RawMadt, interrupt_controller_structs);

        let ics_len = as_cast!(header.length, u32 => usize)
            .checked_sub(skip_bytes).ok_or(AcpiError::CorruptedFields)?;

        let ics_begin = unsafe {
            (&raw const (*ptr).interrupt_controller_structs).cast()
        };

        let ics_sum = unsafe { wrapping_sum_raw_slice(ics_begin, ics_len) };

        if header_sum.wrapping_add(ics_sum) != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        let flags: u32;
        let local_apic_addr: PhysAddr32;

        unsafe {
            flags = (&raw const (*ptr).flags).read_unaligned();
            local_apic_addr = (&raw const (*ptr).local_int_controller_addr)
                .read_unaligned();
        }

        // All bits of the flags field, except the last one, should be zero.
        if (flags & !1) != 0 {
            return Err(AcpiError::CorruptedFields);
        }

        let dual8259 = (flags & 0b1) == 1;

        let madt = Madt {
            header,
            local_apic_addr,
            dual8259,
            ics_begin,
            ics_len,
        };

        Ok(madt)
    }
}
