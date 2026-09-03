use crate::prelude::*;

use core::arch::asm;

/// Global descriptor table register memory location. This is what will be
/// loaded by the `lgdt` instruction to finally flush the GDT.
static mut GDTR: Gdtr = Gdtr { size: 0, base: 0 };

/// Flush the global descriptor table by calling `lgdt` and update the data
/// segments as well.
///
/// # SAFETY
///
/// The correct kernel code and data segment descriptor must be loaded at offset
/// 0x08 and 0x10 respectively.
unsafe extern "C" fn gdt_flush(gdtr: *const Gdtr) {
    // Due to a bug in llvm we need to use att syntax to make a far jump.
    //
    // https://www.reddit.com/r/rust/comments/o8lrz8/how_do_i_get_a_far_absolute_jump_with_inline
    unsafe {
        asm!(r#"
                lgdt    ({gdtr})

                ljmp    $0x08, $2f

            2:
                mov     %ax, %ds
                mov     %ax, %es
                mov     %ax, %fs
                mov     %ax, %gs
                mov     %ax, %ss
            "#,

            gdtr = in(reg) gdtr.addr(),
            in("ax") 0x10,

            options(att_syntax),
        )
    }
}

pub fn get_gdtr_address() -> *const Gdtr {
    &raw const GDTR
}

#[repr(C, packed)]
pub struct Gdtr {
    /// Size of the table in bytes, minus 1.
    size: u16,

    /// Linear address of the GDT, not the physical address..???
    base:  u32,
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

        serial!("[gdt_install]: limit = 0x{:02x}\n", size);
        serial!("[gdt_install]: base  = 0x{:02x}\n", base);

        let ptr = ((&raw mut GDTR) as *mut Gdtr).cast::<u8>();

        unsafe {
            // Gdtr is a packed struct, so pointers to its fields may not be
            // aligned.
            ptr.cast::<u16>().write_unaligned(size);
            ptr.add(2).cast::<u32>().write_unaligned(base);
        }

        unsafe { gdt_flush(&raw const GDTR) };
    }
}

