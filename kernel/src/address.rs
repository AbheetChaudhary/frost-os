#![allow(unused)]

//! Useful kernel abstractions.
//!
//! Normally in the userspace there is just one kind of pointer: the pointer to
//! some virtual memory. Languages don't need to be concerned with the fact that
//! the pointer does not points to an actual location in the RAM but to some
//! location in the virtual address space.
//!
//! But inside the kernel we must make that distinction and design all the
//! function API's with this distinction in mind.
//!
//! This module defines some pointer/pointer-like types for use specifically in
//! the kernel.
//!
//! The main pointer types: `VirtPtr<T>`, `VirtPtrMut<T>`, `PhysPtr<T>` and
//! `PhysPtrMut<T>` are just a wrapper on the Rust `*const T` and `*mut T`
//! pointer types. The only meaning they add is the distinction whether the
//! pointer points to virtual or physical address. They have similar pointer
//! operations defined on them as the Rust pointer types and have all the same
//! safety preconditions and some more.
//!
//! The extra safety preconditions are not fully decided yet but they will be
//! related to the virtual/physical nature of the location they point to. Expect
//! something along the lines:
//!
//! - While trying to read through a virtual pointer, it must have an allocated
//!   page table entry or it must be possible to make a page table entry for 
//!   that pointer if a page-fault occurs.
//!
//! - While trying to write through a `PhysPtrMut` the physical address space
//!   pointer must point to some memory region which is not in one of the 
//!   reserved memory regions and the kernel must be allowed to write to that 
//!   address, etc.

use core::fmt;

// Concerning Safety:
//
// - physical mapped pointer could be null.


use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::mem::MaybeUninit;

#[doc(hidden)]
pub use phys_ptr::{PhysAddr, PhysPtr, PhysPtrMut};

pub mod phys_ptr {
    //! Pointer types for physical addresses on the hardware.
    //!
    //! These are the kernel equivalent of the `*const T` and `*mut T` raw
    //! pointers with the added condition that they are meaningful only in the
    //! hardware physical address space.
    //!
    //! These pointers must not be mixed with virtual address space pointers or
    //! userspace virtual pointers.
    //!
    //! They have similar operations as their respective raw pointer
    //! counterparts with all their safety requirements and some more. Exactly
    //! what extra safety requirements they have is not completely decided yet.

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
    /// first convert it to `PhysPtr<T>` and follow its safety requirements over
    /// reading and writing from physical address space.
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


    /// `*const T` equivalent for the physical address space.
    ///
    /// This is just a `PhysAddr` and a phantom `*const T`.
    #[derive(Clone, Copy)]
    #[repr(transparent)]
    pub struct PhysPtr<T: ?Sized> {
        /// The address of the pointer.
        addr: PhysAddr,

        /// Marker type to make this struct behave like a `*const T`.
        _marker: PhantomData<*const T>,
    }

    impl<T> PhysPtr<T> {
        /// Creates a new `PhysPtr<T>` with given physical address.
        pub const fn from_addr(addr: PhysAddr) -> PhysPtr<T> {
            Self {
                addr,
                _marker: PhantomData,
            }
        }

        /// Creates a new `PhysPtr<T>` by mapping `self`'s physical address to a
        /// new one.
        pub fn map_addr<F>(self, f: F) -> PhysPtr<T>
            where F: FnOnce(PhysAddr) -> PhysAddr
        {
            let new_addr = f(self.addr());

            Self::from_addr(new_addr)
        }

        /// Gets the physical address of the pointer.
        pub const fn addr(self) -> PhysAddr {
            PhysAddr(self.addr.addr())
        }

        /// Gets the `usize` address of the pointer.
        pub const fn address(self) -> usize {
            self.addr.addr()
        }

        pub const fn cast<U>(self) -> PhysPtr<U> {
            PhysPtr::<U>::from_addr(self.addr())
        }

        /* TODO
        pub const fn cast_mut(self) -> PhysPtrMut<T> {
            PhysPtrMut(self.as_ptr().cast_mut())
        }
        */

        /// Offset the physical pointer by `count` units of `T`.
        ///
        /// # Safety
        /// - If the computed offset is non-zero then `self` must be derived
        ///   from some allocation and the resulting pointer must also be in
        ///   bounds of that allocation.
        ///
        /// # Panics
        ///
        /// The function panics if:
        ///
        /// - `count * size_of::<T>() as isize` does not fits inside a `isize`.
        /// - addition of `count * size_of::<T>()` causes an overflow.
        pub const unsafe fn offset(self, count: isize) -> PhysPtr<T> {
            let byte_shift: isize = count.strict_mul(size_of::<T>() as isize);

            Self::from_addr(self.addr().strict_add_signed(byte_shift))
        }

        /// Adds an unsigned offset of `count` units of `T` to the physical
        /// pointer.
        ///
        /// # Safety
        /// - If the computed offset is non-zero then `self` must be derived
        ///   from some allocation and the resulting pointer must also be in
        ///   bounds of that allocation.
        ///
        /// # Panics
        ///
        /// The function panics if:
        ///
        /// - `count * size_of::<T>()` does not fits inside a `usize`.
        /// - addition of `count * size_of::<T>()` causes an overflow.
        pub const unsafe fn add(self, count: usize) -> PhysPtr<T> {
            let byte_shift: usize = count.strict_mul(size_of::<T>());

            Self::from_addr(self.addr().strict_add(byte_shift))
        }

        /// Subtracts an unsigned offset of `count` units of `T` from the
        /// physical pointer.
        ///
        /// # Safety
        /// - If the computed offset is non-zero then `self` must be derived
        ///   from some allocation and the resulting pointer must also be in
        ///   bounds of that allocation.
        ///
        /// # Panics
        ///
        /// The function panics if:
        ///
        /// - `count * size_of::<T>()` does not fits inside a `usize`.
        /// - subtraction of `count * size_of::<T>()` causes an overflow.
        pub const unsafe fn sub(self, count: usize) -> PhysPtr<T> {
            let byte_shift: usize = count.strict_mul(size_of::<T>());

            Self::from_addr(self.addr().strict_sub(byte_shift))
        }

        /// Offset the physical pointer by `count` number of bytes.
        ///
        /// # Safety
        /// - If the computed offset is non-zero then `self` must be derived
        ///   from some allocation and the resulting pointer must also be in
        ///   bounds of that allocation.
        ///
        /// # Panics
        ///
        /// The function panics if:
        ///
        /// - addition of `count` causes an overflow.
        pub const unsafe fn byte_offset(self, count: isize) -> PhysPtr<T> {
            unsafe { self.cast::<u8>().offset(count).cast::<T>() }
        }

        /// Adds an unsigned offset of `count` bytes to the physical pointer.
        ///
        /// # Safety
        /// - If the computed offset is non-zero then `self` must be derived
        ///   from some allocation and the resulting pointer must also be in
        ///   bounds of that allocation.
        ///
        /// # Panics
        ///
        /// The function panics if:
        ///
        /// - addition of `count` causes an overflow.
        pub const unsafe fn byte_add(self, count: usize) -> PhysPtr<T> {
            unsafe { self.cast::<u8>().add(count).cast::<T>() }
        }

        /// Subtracts an unsigned offset of `count` bytes from the physical
        /// pointer.
        ///
        /// # Safety
        /// - If the computed offset is non-zero then `self` must be derived
        ///   from some allocation and the resulting pointer must also be in
        ///   bounds of that allocation.
        ///
        /// # Panics
        ///
        /// The function panics if:
        ///
        /// - subtraction of `count` causes an overflow.
        pub const unsafe fn byte_sub(self, count: usize) -> PhysPtr<T> {
            unsafe { self.cast::<u8>().sub(count).cast::<T>() }
        }

        /// Reads the physical memory pointed to by `self` without moving it.
        ///
        /// # Safety
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` must be valid for reads.
        /// - `self` must be properly aligned, otherwise use `read_unaligned`.
        /// - `self` must point to a properly initialized value of `T`.
        pub unsafe fn read(self) -> T {
            let ptr = core::ptr::with_exposed_provenance::<T>(self.address());

            unsafe {
                ptr.read()
            }
        }

        /// Reads the physical memory pointed to by `self` without moving it.
        ///
        /// # Safety
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` must be valid for reads.
        /// - `self` must point to a properly initialized value of `T`.
        pub unsafe fn read_unaligned(self) -> T {
            let ptr = core::ptr::with_exposed_provenance_mut::<T>(self.address());

            unsafe {
                ptr.read_unaligned()
            }
        }

        /// Performs a volatile read the the physical address pointed to by
        /// `PhysPtr<T>`.
        ///
        /// # Safety
        ///
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` is properly aligned for `T`.
        /// - The physical address is valid for a volatile read on the target
        ///   hardware.
        /// - The read does not trap.
        /// - The read produces a properly initialized value of type `T`.
        /// - The access does not violates any hardware specific requirements.
        /// - The accessed memory is not Rust-allocated memory unless the usual
        ///   Rust rules for volatile access to Rust allocations are satisfied.
        ///
        /// Also read the safety preconditions of `core::ptr::read_volatile`.
        pub unsafe fn read_volatile(self) -> T {
            let ptr = core::ptr::without_provenance::<T>(self.address());

            unsafe {
                ptr.read_volatile()
            }
        }
    }

    /// `*mut T` equivalent for the physical address space.
    #[derive(Clone, Copy)]
    #[repr(transparent)]
    pub struct PhysPtrMut<T: ?Sized> {
        addr: PhysAddr,

        _marker: PhantomData<*mut T>,
    }

    impl<T> PhysPtrMut<T> {
        /// Creates a new `PhysPtrMut<T>` with given physical address.
        pub const fn from_addr(addr: PhysAddr) -> PhysPtrMut<T> {
            Self {
                addr,
                _marker: PhantomData,
            }
        }

        /// Creates a new `PhysPtrMut<T>` by mapping `self`'s physical address
        /// to a new one.
        pub fn map_addr<F>(self, f: F) -> PhysPtrMut<T>
            where F: FnOnce(PhysAddr) -> PhysAddr
        {
            let new_addr = f(self.addr());

            Self::from_addr(new_addr)
        }

        /// Gets the physical address of the pointer.
        pub const fn addr(self) -> PhysAddr {
            self.addr
        }

        /// Gets the `usize` address of the pointer.
        pub const fn address(self) -> usize {
            self.addr.addr()
        }

        pub const fn cast<U>(self) -> PhysPtrMut<U> {
            PhysPtrMut::from_addr(self.addr())
        }

        pub const fn cast_const(self) -> PhysPtr<T> {
            PhysPtr::from_addr(self.addr())
        }

        /// Reads the physical memory pointed to by `self` without moving it.
        ///
        /// # Safety
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` must be valid for reads.
        /// - `self` must be properly aligned, otherwise use `read_unaligned`.
        /// - `self` must point to a properly initialized value of `T`.
        pub unsafe fn read(self) -> T {
            let ptr = core::ptr::with_exposed_provenance::<T>(self.address());

            unsafe {
                ptr.read()
            }
        }

        /// Reads the physical memory pointed to by `self` without moving it.
        ///
        /// # Safety
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` must be valid for reads.
        /// - `self` must point to a properly initialized value of `T`.
        pub unsafe fn read_unaligned(self) -> T {
            let ptr = core::ptr::with_exposed_provenance_mut::<T>(self.address());

            unsafe {
                ptr.read_unaligned()
            }
        }

        /// Performs a volatile read the the physical address pointed to by
        /// `PhysPtr<T>`.
        ///
        /// # Safety
        ///
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` is properly aligned for `T`.
        /// - The physical address is valid for a volatile read on the target
        ///   hardware.
        /// - The read does not trap.
        /// - The read produces a properly initialized value of type `T`.
        /// - The access does not violates any hardware specific requirements.
        /// - The accessed memory is not Rust-allocated memory unless the usual
        ///   Rust rules for volatile access to Rust allocations are satisfied.
        ///
        /// Also read the safety preconditions of `core::ptr::read_volatile`.
        pub unsafe fn read_volatile(self) -> T {
            let ptr = core::ptr::without_provenance::<T>(self.address());

            unsafe {
                ptr.read_volatile()
            }
        }

        /// Writes to the physical memory pointed to by `self` without dropping
        /// the old value.
        ///
        /// # Safety
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` must be valid for writes.
        /// - `self` must be properly aligned, otherwise use `write_unaligned`.
        /// - `self` must point to a properly initialized value of `T`.
        pub unsafe fn write(self, val: T) {
            let ptr = core::ptr::with_exposed_provenance_mut::<T>(self.address());

            unsafe {
                ptr.write(val)
            }
        }

        /// Writes to the physical memory pointed to by `self` without dropping
        /// the old value.
        ///
        /// # Safety
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` must be valid for writes.
        /// - `self` must point to a properly initialized value of `T`.
        pub unsafe fn write_unaligned(self, val: T) {
            let ptr = core::ptr::with_exposed_provenance_mut::<T>(self.address());

            unsafe {
                ptr.write_unaligned(val)
            }
        }

        /// Performs a volatile write at the physical address pointed to by
        /// `PhysPtrMut<T>`.
        ///
        /// # Safety
        ///
        /// - Paging must be disabled, or the page containing the address must
        ///   be identity mapped.
        /// - `self` is properly aligned for `T`, even for ZST.
        /// - The physical address is valid for a volatile write on the target
        ///   hardware.
        /// - The write does not trap.
        /// - The access does not violates any hardware specific requirements.
        /// - The accessed memory is not Rust-allocated memory unless the usual
        ///   Rust rules for volatile access to Rust allocations are satisfied.
        pub unsafe fn write_volatile(self, val: T) {
            let ptr = core::ptr::without_provenance_mut::<T>(self.address());
            unsafe {
                ptr.write_volatile(val)
            }
        }

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub fn new(addr: usize) -> VirtAddr {
        Self(addr)
    }

    pub fn addr(self) -> usize {
        self.0
    }
}
