use crate::address;

use address::{PhysAddr};

#[repr(usize)]
pub enum FrameKind {
    FourK  = 1 << 12,

    _TwoMeg = 1 << 21,
    _OneGig = 1 << 30,
}

pub struct PhysFrame {
    addr: PhysAddr,
    kind: FrameKind,
}

impl PhysFrame {
    pub fn from_addr_4k(addr: PhysAddr) -> Option<PhysFrame> {
        if !addr.is_aligned_to(1<<12) {
            return None;
        }

        let frame = Self {
            addr,
            kind: FrameKind::FourK,
        };

        Some(frame)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PhysMemType {
    AcpiMemoryNvs,
    AcpiReclaimMemory,

    RuntimeServicesCode,
    RuntimeServicesData,

    UnusedMemory,

    KernelCode,
    KernelStack,
    KernelData,
    KernelRodata,

    BiosRegion,

    ReservedMemoryType,
}
