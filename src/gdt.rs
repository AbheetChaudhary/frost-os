use crate::{println};

#[repr(C, packed)]
pub struct GDTR {
    /// Size of the table in bytes, minus 1.
    size: u16,

    /// Linear address of the GDT, not the physical address..???
    base:  u32,
}

unsafe extern "C" {
    static mut _gdtr: GDTR;

    fn _gdt_flush();
}

// Refer osdev website.
#[repr(C)]
pub struct SegmentDescriptor {
    base:   u32,
    limit:  u32,
    access: u8,
    flags:  u8,
}

/// Size of each GDT entry in bytes.
const GDT_ENTRY_SIZE: usize = 8;

/// Some space reserved for the global descriptor table. Reserved for 256
/// entries. GDT could contain space for 8192 entries at max.
static mut GLOBAL_DESC_TABLE: [u8; 256 * GDT_ENTRY_SIZE] = [0u8; _];

#[derive(Default)]
pub struct GDT {
    /// Number of entries already inserted into the table.
    entries: usize,
}

impl GDT {
    /// Create a pointer to the first byte of the GLOBAL_DESC_TABLE without
    /// creating any intermediate references.
    fn _global_desc_ptr() -> *mut u8 {
        (&raw mut GLOBAL_DESC_TABLE).cast()
    }

    pub fn insert_next_entry(&mut self, source: SegmentDescriptor) {
        if source.limit > 0xFFFFF {
            panic!("GDT cannot encode limits larger than 0xFFFFF");
        }

        // Base for the next entry.
        let base = self.entries * 8;

        unsafe {
            GLOBAL_DESC_TABLE[base + 0] = (source.limit & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 1] = ((source.limit >> 8) & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 6] = ((source.limit >> 16) & 0x0F) as u8;

            GLOBAL_DESC_TABLE[base + 2] = (source.base & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 3] = ((source.base >> 8) & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 4] = ((source.base >> 16) & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 7] = ((source.base >> 24) & 0xFF) as u8;

            GLOBAL_DESC_TABLE[base + 5] = source.access;

            GLOBAL_DESC_TABLE[base + 6] |= source.flags << 4;
        }

        self.entries += 1;
    }

    pub fn update_entry(&mut self, idx: usize, source: SegmentDescriptor) {
        if source.limit > 0xFFFFF {
            panic!("GDT cannot encode limits larger than 0xFFFFF");
        }

        let base = idx * 8;

        unsafe {
            GLOBAL_DESC_TABLE[base + 0] = (source.limit & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 1] = ((source.limit >> 8) & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 6] = ((source.limit >> 16) & 0x0F) as u8;

            GLOBAL_DESC_TABLE[base + 2] = (source.base & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 3] = ((source.base >> 8) & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 4] = ((source.base >> 16) & 0xFF) as u8;
            GLOBAL_DESC_TABLE[base + 7] = ((source.base >> 24) & 0xFF) as u8;

            GLOBAL_DESC_TABLE[base + 5] = source.access;

            GLOBAL_DESC_TABLE[base + 6] |= source.flags << 4;
        }
    }

    pub fn push_default_null_segment(&mut self) {
        assert!(self.entries == 0, "Default Null segment entry could be inserted only \
            as the first entry. There are alerady {} entries", self.entries);

        let null_descriptor = SegmentDescriptor {
            base:   0,
            limit:  0,
            access: 0,
            flags:  0,
        };

        self.insert_next_entry(null_descriptor);
    }

    pub fn push_default_code_segment(&mut self) {
        assert!(self.entries == 1, "Default Code segment entry could be inserted only \
            as the second entry. There are alerady {} entries", self.entries);

        let code_descriptor = SegmentDescriptor {
            base:   0,
            limit:  0xFFFFF,

            // 0x9A -> Accessed bit is clear
            // 0x9B -> Accesed bit is set
            access: 0x9B,

            // 4KiB Granularity, 32-bit protected mode segment, no-long mode
            flags:  0x0C,
        };

        self.insert_next_entry(code_descriptor);
    }

    pub fn push_default_data_segment(&mut self) {
        assert!(self.entries == 2, "Default Data segment entry could be inserted only \
            as the data entry. There are alerady {} entries", self.entries);

        let data_descriptor = SegmentDescriptor {
            base:   0,
            limit:  0xFFFFF,

            // 0x92 -> Accessed bit is clear
            // 0x93 -> Accesed bit is set
            access: 0x93,

            // 4KiB Granularity, 32-bit protected mode segment, no-long mode
            flags:  0x0C,
        };

        self.insert_next_entry(data_descriptor);
    }

    pub fn gdt_install(&self) {
        let size = 8 * (self.entries as u16) - 1;
        let base = &raw const GLOBAL_DESC_TABLE as u32;

        println!("[gdt_install]: limit = 0x{:02x}", size);
        println!("[gdt_install]: base  = 0x{:02x}", base);

        let ptr = ((&raw mut _gdtr) as *mut GDTR).cast::<u8>();

        unsafe {
            // GDTR is a packed struct, so pointers to its fields may not be
            // aligned.
            ptr.cast::<u16>().write_unaligned(size);
            ptr.add(2).cast::<u32>().write_unaligned(base);
        }

        unsafe { _gdt_flush() };
    }
}

