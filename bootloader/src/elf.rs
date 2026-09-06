
use core::fmt;

use crate::mm;

use mm::{VirtAddr, Address};

/*
const PERM_READ:  u8 = 1 << 2;
const PERM_WRITE: u8 = 1 << 1;
const PERM_EXEC:  u8 = 1 << 0;
*/

const X86_MACHINE: u16 = 0x03;
// const AMD64_MACHINE: u16 = 0x3e;

#[derive(Copy, Clone)]
pub struct Loadable {
    pub offset:    usize,
    pub virt_addr: VirtAddr,
    pub file_sz:   usize,
    pub mem_sz:    usize,
    pub perm:      u8, // RWX
}

impl fmt::Debug for Loadable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Loadable")
            .field("offset", &format_args!("0x{:x}", self.offset))
            .field("virt_addr", &format_args!("0x{:x}", self.virt_addr.addr()))
            .field("file_sz", &format_args!("0x{:x}", self.file_sz))
            .field("mem_sz", &format_args!("0x{:x}", self.mem_sz))
            .field("perm", &format_args!("0b{:b}", self.perm))
            .finish()
    }
}

fn validate_header(file: &[u8]) -> Option<()> {
    // Ensure enough bytes to read e_ident field.
    if file.len() < 16 {
        return None;
    }
    
    let mut e_ident = [0u8; 16];
    e_ident.copy_from_slice(&file[..16]);

    // Confirm ELF magic bytes.
    if &e_ident[0..4] != &[0x7f, 0x45, 0x4c, 0x46] {
        return None;
    }

    // Ensure 32-bit binary.
    if e_ident[4] != 1 {
        return None;
    }

    // Ensure little endian.
    if e_ident[5] != 1 {
        return None;
    }

    // Check elf version.
    if e_ident[6] != 1 {
        return None;
    }

    Some(())
}

/// Validate that the file is an executable. Expects a little endian binary
/// file.
fn validate_executable_type(file: &[u8]) -> Option<()> {
    if file.len() < 18 {
        return None;
    }

    let mut e_type = [0u8; 2];

    e_type.copy_from_slice(&file[16..18]);

    // Expected little endian.
    let e_type = u16::from_le_bytes(e_type);

    // 0x02 == ET_EXEC
    if e_type != 0x02 {
        return None;
    }

    Some(())
}

/// Validates whether the elf file is of the expected architecture.
///
/// Expects a little endian file.
fn validate_arch(file: &[u8], arch: u16) -> Option<()> {
    if file.len() < 20 {
        return None;
    }

    let mut e_machine = [0u8; 2];
    e_machine.copy_from_slice(&file[18..20]);

    // Expected LE bytes.
    let e_machine = u16::from_le_bytes(e_machine);

    if e_machine != arch {
        return None;
    }

    Some(())
}

// Gets the three loadable sections from the file.
pub fn get_loadables_and_entry(file: &[u8]) -> Option<([Loadable; 3], usize)> {
    validate_header(file)?;
    validate_executable_type(file)?;
    validate_arch(file, X86_MACHINE)?;

    // Typical 32-bit header size is 0x34. This ensures we don't have to do size
    // checks for e_entry, e_phoff, e_phnum.
    if file.len() < 0x34 {
        return None;
    }

    // Already checked x86 machine.
    let mut e_entry = [0u8; 4];
    e_entry.copy_from_slice(&file[24..28]);

    let e_entry = u32::from_ne_bytes(e_entry);
    let e_entry: usize = e_entry.try_into().ok()?;

    // Get program header offset.
    let e_phoff: usize = 0x34; // Typical for 32-bit binaries, which it is.

    let e_phentsize: usize = 0x20; // Typical for 32-bit binaries.

    // Get the number of entries in the program header.
    let mut e_phnum = [0u8; 2];
    e_phnum.copy_from_slice(&file[0x2c..0x2e]);

    let e_phnum = u16::from_ne_bytes(e_phnum) as usize;

    // Make sure that there are enough bytes for all the program headers after
    // the elf header.
    if file[e_phoff..].len() < e_phnum * e_phentsize {
        return None;
    }

    let _dummy = Loadable {
        offset:    0,
        virt_addr: VirtAddr::new(0),
        file_sz:   0,
        mem_sz:    0,
        perm:      0, // RWX
    };

    // The three loadable segments that will be returned.
    let mut loadables = [_dummy; 3];

    // Number of loadable segments that have already been initialized in
    // loadables above.
    let mut l_count = 0;

    for i in 0..e_phnum {
        // File offset at which this program header begins.
        let ph_begin = e_phoff + i * e_phentsize;

        // The program header.
        let ph = &file[ph_begin..ph_begin + e_phentsize];

        // Type of the program header.
        let mut p_type = [0u8; 4];
        p_type.copy_from_slice(&ph[0..4]);
        let p_type = u32::from_ne_bytes(p_type);

        const PT_LOAD: u32 = 0x0000_0001;

        // Skip this segment if it is not a loadable.
        if p_type != PT_LOAD {
            continue;
        }

        // Load offset of segment in the file.
        let p_offset = usize::from_ne_bytes(ph[0x04..0x08].try_into().ok()?);

        // Virtual address at which the segment wants to be loaded.
        let p_vaddr = usize::from_ne_bytes(ph[0x08..0x0c].try_into().ok()?);

        // File size(in bytes) that this section occupies.
        let p_filesz = usize::from_ne_bytes(ph[0x10..0x14].try_into().ok()?);

        // If the file does not have enough bytes to accomodate this section
        // then early return.
        if file[p_offset..].len() < p_filesz {
            return None;
        }

        // Size this section occupies in memory.
        let p_memsz = usize::from_ne_bytes(ph[0x14..0x18].try_into().ok()?);

        // Permission flags for this section.
        let p_flags = u32::from_ne_bytes(ph[0x18..0x1c].try_into().ok()?);

        // Flags something other than R|W|X.
        if p_flags > 0b111 {
            return None;
        }

        let p_flags: u8 = p_flags.try_into().ok()?;

        // Section alignment.
        let p_align = usize::from_ne_bytes(ph[0x1c..0x20].try_into().ok()?);

        assert!(p_align % (1<<12) == 0);

        let loadable = Loadable {
            offset:    p_offset,
            virt_addr: VirtAddr::new(p_vaddr),
            file_sz:   p_filesz,
            mem_sz:    p_memsz,
            perm:      p_flags,
        };

        loadables[l_count] = loadable;

        l_count += 1;
        if l_count == 3 {
            break;
        }
    }

    Some((loadables, e_entry))
}
