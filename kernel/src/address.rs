//! Useful kernel abstractions around physical and virtual address and address
//! ranges.

use core::fmt;

#[doc(hidden)]
pub use phys_ptr::{PhysAddr};

pub mod phys_ptr {
    use super::*;

    /// An address in the physical address space with no type attached to it.
    ///
    /// This is like the underlying `usize` address of raw pointers, with the
    /// added meaning that this address is in the physical address space.
    /// Priamarily useful for doing arithmetics on physical addresses.
    ///
    /// All arithmetic operations on `PhysAddr` guarantee non-wrapping
    /// behaviour. Operations that cannot be represented either return
    /// `None` (`checked_*`) or panic (`strict_*`).
    ///
    /// A `PhysAddr` does not necessarily mean that it points to a valid
    /// physical address. It is just an abstraction over a `usize` with some
    /// methods that are helpful in arithmetic over addresses. When actually
    /// trying to read the memory from the physical address one does need to
    /// satisfy the safety requirements over reading and writing from physical
    /// address space.
    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(transparent)]
    pub struct PhysAddr(#[doc(hidden)] usize);

    impl fmt::Debug for PhysAddr {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "PhysAddr({:p})", self.addr() as *const ())
        }
    }

    impl PhysAddr {
        /// Create a new physical address from a raw `usize`.
        pub const fn new(addr: usize) -> PhysAddr {
            Self(addr)
        }

        /// Get the raw address as `usize`.
        pub const fn addr(self) -> usize {
            self.0
        }

        /// Strict addition of an integer with physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub const fn strict_add(self, count: usize) -> PhysAddr {
            match self.addr().checked_add(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_add: integer overflow occured"),
            }
        }

        /// Checked addition of an integer with physical address. Computes the
        /// new physical address or returns `None` if overflow occurs.
        pub const fn checked_add(self, count: usize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_add(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Strict addition of a signed integer with physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub const fn strict_add_signed(self, count: isize) -> PhysAddr {
            match self.addr().checked_add_signed(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_add_signed: integer overflow occured"),
            }
        }

        /// Checked addition of a signed integer with physical address. Computes
        /// the new physical address or returns `None` if overflow occurs.
        pub const fn checked_add_signed(self, count: isize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_add_signed(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Strict subtraction of an integer from a physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub const fn strict_sub(self, count: usize) -> PhysAddr {
            match self.addr().checked_sub(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_sub: integer overflow occured"),
            }
        }

        /// Checked subtraction of an integer from a physical address. Computes
        /// the new physical address or returns `None` if overflow occurs.
        pub const fn checked_sub(self, count: usize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_sub(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Strict subtraction of a signed integer from a physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub const fn strict_sub_signed(self, count: isize) -> PhysAddr {
            match self.addr().checked_sub_signed(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_sub_signed: integer overflow occured"),
            }
        }

        /// Checked subtraction of a signed integer from a physical address.
        /// Computes the new physical address or returns `None` if overflow
        /// occurs.
        pub const fn checked_sub_signed(self, count: isize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_sub_signed(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Checks whether the address is aligned to `align`. Returns `true` if
        /// it is aligned otherwise return `false`.
        ///
        /// # Panics
        ///
        /// The function panics if `align` is not a power-of-two.
        pub const fn is_aligned_to(self, align: usize) -> bool {
            if !align.is_power_of_two() {
                panic!("is_aligned_to: align is not a power-of-two");
            }

            (self.addr() & (align - 1)) == 0
        }

        /// Generate a new physical address less than or equal to `self` that is
        /// aligned to `align`.
        ///
        /// # Panics
        ///
        /// The function panics if `align` is not a power-of-two.
        pub const fn align_down(self, align: usize) -> PhysAddr {
            if !align.is_power_of_two() {
                panic!("align_down: align is not a power-of-two");
            }

            let addr = self.addr() & !(align - 1);

            Self(addr)
        }

        /// Generates a new physical address greater than or equal to `self`
        /// that is aligned to `align`. If the alignment could not be achieved
        /// without overflow then returns `None`.
        ///
        /// # Panics
        ///
        /// The function panics if `align` is not a power-of-two.
        pub const fn checked_align_up(self, align: usize) -> Option<PhysAddr> {
            if !align.is_power_of_two() {
                panic!("checked_align_up: align is not a power-of-two");
            }

            // Remainder upon dividing addr with align.
            let rem = self.addr() & (align - 1);

            // If address is already aligned, do nothing.
            if rem == 0 {
                return Some(self);
            }

            // Amount that should be added to addr to make it a multiple of
            // align.
            let to_add = align - rem;

            match self.addr().checked_add(to_add) {
                Some(aligned_up_addr) => Some(Self(aligned_up_addr)),
                None => None,
            }
        }

        /*
        /// Fill `buffer` by reading bytes from physical address `self`.
        ///
        /// # Panics
        ///
        /// The function panics if `self` is too close to the end of physical
        /// address space such that trying to read `buffer.len()` number of
        /// bytes causes wrapping around the physical address space.
        ///
        /// # Safety
        ///
        /// - `self` must be safe for volatile reads.
        pub unsafe fn read_into_buffer_strict(self, buffer: &mut [MaybeUninit<u8>]) {
            let len = buffer.len();
            let mut addr = self.addr();

            // Number of bytes read till now.
            let mut i = 0;

            // Read individual bytes until we get an alignment of 4.
            while i < len && (addr.strict_add(i) % 4) != 0 {
                let b = unsafe { self.strict_add(i).read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }

            // Read as many chunks of 4 as we can.
            // 
            // Index of the next byte that we are going to read, i.e. i and then
            // 3 bytes after that must be less than len.
            while i + 3 < len {
                let chunk = unsafe { self.strict_add(i).read_u32() };
                let bytes = chunk.to_ne_bytes();

                for j in 0..4 {
                    buffer[i + j].write(bytes[j]);
                }

                i += 4;
            }

            // Read remaining bytes.
            while i < len {
                let b = unsafe { self.strict_add(i).read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }
        }

        /// Fill `buffer` byte reading bytes from physical address `self`.
        ///
        /// Returns `None` if `self` was too close to the end of the physical
        /// address space, such that trying to read `buffer.len()` number of
        /// bytes would have caused an integer overflow.
        ///
        /// # Safety
        ///
        /// - `self` must be safe for volatile reads.
        pub unsafe fn read_into_buffer_checked(self, buffer: &mut [MaybeUninit<u8>])
            -> Option<()> {

            let len = buffer.len();
            let mut addr = self.addr();

            // Number of bytes read till now.
            let mut i = 0;

            // Read individual bytes until we get an alignment of 4.
            while i < len && (addr.checked_add(i)? % 4) != 0 {
                let b = unsafe { self.checked_add(i)?.read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }

            // Read as many chunks of 4 as we can.
            // 
            // Index of the next byte that we are going to read, i.e. i and then
            // 3 bytes after that must be less than len.
            while i + 3 < len {
                let chunk = unsafe { self.checked_add(i)?.read_u32() };
                let bytes = chunk.to_ne_bytes();

                for j in 0..4 {
                    buffer[i + j].write(bytes[j]);
                }

                i += 4;
            }

            // Read remaining bytes.
            while i < len {
                let b = unsafe { self.checked_add(i)?.read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }

            Some(())
        }
        */
    }
}

/// A continuous half-open range of physical addresses.
///
/// This could be an empty range.
///
/// # Note
///
/// This method of representing a range can never represent the range that
/// contains the last address in the address space. One past the last address
/// will wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysRange {
    /// The start of the address range(inclusive).
    start: PhysAddr,

    last: PhysAddr,
}

impl PhysRange {
    pub fn from_start_and_last_checked(start: PhysAddr, last: PhysAddr) -> Option<PhysRange> {
        if last < start || (start.addr() == 0 && last.addr() == usize::MAX) {
            return None;
        }

        let range = Self {
            start,
            last,
        };

        Some(range)
    }

    /// # Panics
    pub fn from_start_and_last(start: PhysAddr, last: PhysAddr) -> PhysRange {
        match Self::from_start_and_last_checked(start, last) {
            Some(r) => r,
            None => {
                if start.addr() == 0 {
                    panic!("Cursed Range!!!");
                } else {
                    panic!("last < start");
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        // This will panic only in case of last < start, or using the cursed
        // range.
        self.last.addr().strict_sub(self.start.addr()).strict_add(1)
    }

    // There are no empty ranges
    /*
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    */

    pub fn start(&self) -> PhysAddr {
        self.start
    }

    pub fn last(&self) -> PhysAddr {
        self.last
    }

    pub fn start_addr(&self) -> usize {
        self.start.addr()
    }

    pub fn last_addr(&self) -> usize {
        self.last.addr()
    }

    pub fn contains(&self, addr: &PhysAddr) -> bool {
        *addr >= self.start && *addr <= self.last
    }

    pub fn contains_range(&self, other: &PhysRange) -> bool {
        self.start <= other.start && self.last >= other.last
    }

    pub fn overlap(&self, other: &PhysRange) -> Option<PhysRange> {
        let start = self.start.max(other.start);
        let last = self.last.min(other.last);

        if start <= last {
            let range = Self {
                start,
                last,
            };

            Some(range)
        } else {
            None
        }
    }

    pub fn is_left_adjacent_to(&self, other: &PhysRange) -> bool {
        self.last < other.start && (self.last.strict_add(1) == other.start)
    }

    pub fn is_right_adjacent_to(&self, other: &PhysRange) -> bool {
        other.last < self.start && (other.last.strict_add(1) == self.start)
    }

    pub fn is_adjacent(&self, other: &PhysRange) -> bool {
        self.is_left_adjacent_to(other) || self.is_right_adjacent_to(other)
    }
}


/// A 64-bit physical address.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr64(u64);

impl PhysAddr64 {
    /// Create a new 64-bit physical address.
    pub const fn new(addr: u64) -> PhysAddr64 {
        Self(addr)
    }

    /// Get the address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// A 32-bit physical address.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr32(u32);

impl PhysAddr32 {
    /// Create a new 32-bit physical address.
    pub const fn new(addr: u32) -> PhysAddr32 {
        Self(addr)
    }

    /// Get the address as a `u32`.
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl TryFrom<PhysAddr64> for PhysAddr {
    type Error = core::num::TryFromIntError;

    fn try_from(val: PhysAddr64) -> Result<Self, Self::Error> {
        val.as_u64().try_into().map(Self::new)
    }
}

impl TryFrom<PhysAddr32> for PhysAddr {
    type Error = core::num::TryFromIntError;

    fn try_from(val: PhysAddr32) -> Result<Self, Self::Error> {
        val.as_u32().try_into().map(Self::new)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl VirtAddr {
    pub const fn new(addr: usize) -> VirtAddr {
        Self(addr)
    }

    pub const fn addr(self) -> usize {
        self.0
    }

    /// # Panics
    pub const fn strict_add(self, count: usize) -> VirtAddr {
        Self::new(self.addr().strict_add(count))
    }

    pub const fn strict_sub(self, count: usize) -> VirtAddr {
        Self::new(self.addr().strict_sub(count))
    }

    /// Checks whether the address is aligned to `align`. Returns `true` if
    /// it is aligned otherwise return `false`.
    ///
    /// # Panics
    ///
    /// The function panics if `align` is not a power-of-two.
    pub const fn is_aligned_to(self, align: usize) -> bool {
        if !align.is_power_of_two() {
            panic!("is_aligned_to: align is not a power-of-two");
        }

        (self.addr() & (align - 1)) == 0
    }

    pub fn map_addr<F>(self, f: F) -> VirtAddr
    where
        F: FnOnce(usize) -> usize
    {
        Self::new(f(self.addr()))
    }

    pub const fn as_ptr<T>(self) -> *const T {
        let ptr: *const u8 = core::ptr::with_exposed_provenance(self.addr());

        ptr.cast()
    }

    pub const fn as_mut<T>(self) -> *mut T {
        let ptr: *mut u8 = core::ptr::with_exposed_provenance_mut(self.addr());

        ptr.cast()
    }
}
