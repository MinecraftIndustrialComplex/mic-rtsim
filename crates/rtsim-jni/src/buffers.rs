// SPDX-License-Identifier: AGPL-3.0-or-later
//! The four boundary buffers (docs/boundary.md §1): engine allocations, 64-byte aligned, that
//! Java sees as direct `ByteBuffer`s over the same memory.
//!
//! **Who touches what, when.** Only the two sequence words of a header are ever accessed by both
//! sides at once, and only as atomics ([`Buf::published`], [`Buf::consumed`]; acquire loads and
//! release stores, matching Java's `byteBufferViewVarHandle` `getAcquire`/`setRelease`). Every
//! other byte belongs to one side at a time, handed over by those words: an observation buffer is
//! Java's until it publishes hand-over h, then the step's until it stores `consumed = h`; a command
//! buffer is the step's once Java stored `consumed = h - 2`, then Java's once the step publishes h.
//! [`Buf::area`] and [`Buf::area_mut`] are `unsafe` for that reason: the caller asserts it holds
//! the buffer by the protocol.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::wire::{BUFFER_ALIGN, H_CAPACITY, H_CONSUMED, H_PUBLISHED, HEADER_BYTES, RECORD_ALIGN};

/// One boundary buffer: a 64-byte header, then `capacity` bytes of records.
pub struct Buf {
    ptr: NonNull<u8>,
    layout: Layout,
}

// SAFETY: the memory is a plain allocation owned by this value; concurrent access follows the
// protocol in the module docs (atomics for the shared words, ownership hand-over for the rest).
unsafe impl Send for Buf {}
unsafe impl Sync for Buf {}

impl Buf {
    /// Allocates a zeroed buffer with `capacity` bytes of record area and writes the capacity
    /// into its header.
    pub fn new(capacity: usize) -> Buf {
        assert!(
            capacity > 0
                && capacity.is_multiple_of(RECORD_ALIGN)
                && capacity <= u32::MAX as usize - HEADER_BYTES,
            "record area must be a positive multiple of 8 below 4 GiB: {capacity}"
        );
        let layout =
            Layout::from_size_align(HEADER_BYTES + capacity, BUFFER_ALIGN).expect("buffer layout");
        // SAFETY: the layout has a non-zero size.
        let raw = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(raw).unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
        let buf = Buf { ptr, layout };
        buf.put_u32(H_CAPACITY, capacity as u32);
        buf
    }

    /// The base address, 64-byte aligned.
    pub fn base(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Header plus record area, in bytes: what the Java `ByteBuffer` spans.
    pub fn total(&self) -> usize {
        self.layout.size()
    }

    /// Bytes of record area.
    pub fn capacity(&self) -> usize {
        self.layout.size() - HEADER_BYTES
    }

    fn word(&self, off: usize) -> &AtomicU64 {
        debug_assert!(off.is_multiple_of(8) && off + 8 <= HEADER_BYTES);
        // SAFETY: an 8-aligned offset inside the allocation (the base is 64-aligned), which Java
        // only touches with atomic accesses of the same width.
        unsafe { &*(self.base().add(off) as *const AtomicU64) }
    }

    /// `published`: release-stored by the producer, acquire-loaded by the consumer.
    pub fn published(&self) -> &AtomicU64 {
        self.word(H_PUBLISHED)
    }

    /// `consumed`: release-stored by the consumer, acquire-loaded by the producer.
    pub fn consumed(&self) -> &AtomicU64 {
        self.word(H_CONSUMED)
    }

    /// Zeroes both sequence words (a new session: hand-overs restart at 1).
    pub fn reset_words(&self) {
        self.published().store(0, Ordering::Release);
        self.consumed().store(0, Ordering::Release);
    }

    pub fn get_u32(&self, off: usize) -> u32 {
        assert!(off + 4 <= HEADER_BYTES);
        // SAFETY: inside the header; the protocol makes this side the field's only accessor now.
        u32::from_le(unsafe { std::ptr::read_unaligned(self.base().add(off) as *const u32) })
    }

    pub fn get_u64(&self, off: usize) -> u64 {
        assert!(off + 8 <= HEADER_BYTES);
        // SAFETY: as get_u32.
        u64::from_le(unsafe { std::ptr::read_unaligned(self.base().add(off) as *const u64) })
    }

    pub fn get_i64(&self, off: usize) -> i64 {
        self.get_u64(off) as i64
    }

    pub fn put_u32(&self, off: usize, v: u32) {
        assert!(off + 4 <= HEADER_BYTES);
        // SAFETY: as get_u32.
        unsafe { std::ptr::write_unaligned(self.base().add(off) as *mut u32, v.to_le()) }
    }

    pub fn put_u64(&self, off: usize, v: u64) {
        assert!(off + 8 <= HEADER_BYTES);
        // SAFETY: as get_u32.
        unsafe { std::ptr::write_unaligned(self.base().add(off) as *mut u64, v.to_le()) }
    }

    pub fn put_i64(&self, off: usize, v: i64) {
        self.put_u64(off, v as u64);
    }

    /// The first `used` bytes of the record area.
    ///
    /// # Safety
    /// The caller holds the buffer by the protocol (no other thread writes it) for as long as the
    /// slice lives.
    pub unsafe fn area(&self, used: usize) -> &[u8] {
        assert!(used <= self.capacity());
        // SAFETY: inside the allocation; exclusivity is the caller's contract.
        unsafe { std::slice::from_raw_parts(self.base().add(HEADER_BYTES), used) }
    }

    /// The whole record area, to write.
    ///
    /// # Safety
    /// The caller holds the buffer by the protocol (no other thread reads or writes it) for as
    /// long as the slice lives, and makes no other slice of it meanwhile.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn area_mut(&self) -> &mut [u8] {
        // SAFETY: inside the allocation; exclusivity is the caller's contract.
        unsafe { std::slice::from_raw_parts_mut(self.base().add(HEADER_BYTES), self.capacity()) }
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        // SAFETY: allocated in `new` with this layout.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_with_capacity_in_the_header() {
        let b = Buf::new(4096);
        assert_eq!(b.base() as usize % 64, 0);
        assert_eq!(b.total(), 64 + 4096);
        assert_eq!(b.get_u32(H_CAPACITY), 4096);
        b.published().store(7, Ordering::Release);
        assert_eq!(
            b.get_u64(H_PUBLISHED),
            7,
            "the atomic word is the little-endian header field"
        );
        b.reset_words();
        assert_eq!(b.published().load(Ordering::Acquire), 0);
    }
}
