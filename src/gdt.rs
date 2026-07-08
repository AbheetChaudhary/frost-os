use core::arch::asm;

use crate::vga_buffer::SpinLock;

#[repr(C, packed)]
struct GDTR {
    limit: u16,
    base:  u32,
}

unsafe extern "C" {
    static mut _gdtr: GDTR;
}

#[repr(C)]
pub struct SegmentDescriptor {
    base:   u32,
    limit:  u32,
    access: u8,
    flags:  u8,
}

static mut GLOBAL_DESC_TABLE: [u8; 256 * 8] = [0u8; _]; // Keep space for 256 entries.

static GLOBAL_GDT: SpinLock<GDT> = SpinLock::new(GDT { entries: 0 });

#[derive(Default)]
pub struct GDT {
    entries: usize,
}

impl GDT {
    pub fn add_entry(&mut self, idx: usize, source: SegmentDescriptor) {
        assert!(self.entries() == idx, "Expected next entry to be at {}. Use \
            `update_entry` if not adding a new entry to the table.", self.entries());

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

            GLOBAL_DESC_TABLE[base + 6] |= (source.flags << 4);
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

            GLOBAL_DESC_TABLE[base + 6] |= (source.flags << 4);
        }
    }

    pub fn add_default_null_segment(&mut self) {
        let null_descriptor = SegmentDescriptor {
            base:   0,
            limit:  0,
            access: 0,
            flags:  0,
        };

        self.add_entry(0, null_descriptor);
    }

    pub fn add_default_code_segment(&mut self) {
        let code_descriptor = SegmentDescriptor {
            base:   0,
            limit:  0xFFFFF,
            access: 0x9A,
            flags:  0xC,
        };

        self.add_entry(1, code_descriptor);
    }

    pub fn add_default_data_segment(&mut self) {
        let data_descriptor = SegmentDescriptor {
            base:   0,
            limit:  0xFFFFF,
            access: 0x92,
            flags:  0xC,
        };

        self.add_entry(2, data_descriptor);
    }

    pub fn entries(&self) -> usize {
        self.entries
    }

    pub fn gdt_flush(&self) {
        let entries = self.entries();

        unsafe {
            _gdtr.limit = 8 * entries as u16 - 1;
            _gdtr.base  = &raw const GLOBAL_DESC_TABLE as u32;

            asm!(
                "lgdt ({})",

                "ljmp $0x08, $2f",
            "2:",
                "mov $0x10, %ax",
                "mov %ax, %ds",
                "mov %ax, %es",
                "mov %ax, %fs",
                "mov %ax, %gs",
                "mov %ax, %ss",

                in(reg) (&raw const _gdtr),
                out("ax") _,
                options(att_syntax),
            );
        }
    }
}

