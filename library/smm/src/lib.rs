//! Secure Memory Management Core Traits
//!
//! Abstractions for TEE memory management and hardware isolation.
//! Defines **SecMemManager** (orchestration), **SecMemAllocator** (allocation logic),
//! and **SecMemProtector** (hardware HAL) for **RT** and **APP** enclaves.
//! Serves as the base for Penglai/Keystone-style secure memory implementations.
#![no_std]
extern crate alloc;
use crate::allocators::NoneAlloc;
use core::ptr::NonNull;
use core::{alloc::Layout, usize};
use log::error;
use pmpm::MAX_PMP_ENTRY_COUNT;

pub mod allocators;
pub mod manager;
pub mod protector;

#[derive(Clone, Copy, PartialEq)]
pub enum SecMemAllocatorWrapper<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    None,
    General,
    Runtime(AR),
    Application(AA),
}

#[derive(Clone, Copy, PartialEq)]
pub enum SecMemType {
    None,
    General,
    Runtime,
    Application,
}

#[derive(Clone, Copy)]
struct SecMemRegion<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    addr: usize,
    len: usize,
    id: usize,
    slot: u32,
    is_used: bool,
    pub allocator: SecMemAllocatorWrapper<ORDER, AR, AA>,
}

pub trait SecMemAllocator<const ORDER: usize> {
    /// Create new allocator
    fn new() -> Self;
    /// Init allocator with specific memory region.
    fn init(&mut self, addr: usize, len: usize) {
        let _ = (addr, len);
    }
    /// Alloc mem from allocator.
    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        let _ = layout;
        Err(())
    }
    /// Free mem to allocator.
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let _ = (ptr, layout);
    }
    /// Returns an upper bound on bytes available for another allocation.
    /// A particular layout may still fail because of alignment or fragmentation.
    fn available(&self) -> usize {
        0
    }
    /// Total mem.
    fn total(&self) -> usize {
        0
    }
}

impl<const ORDER: usize, AR, AA> SecMemRegion<ORDER, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    /// Create new region
    pub fn new(addr: usize, len: usize, id: usize, pmp_slot: u32) -> Self {
        Self {
            addr,
            len,
            id,
            slot: pmp_slot,
            is_used: false,
            allocator: SecMemAllocatorWrapper::General,
        }
    }

    /// If target region is overlap with current mem region.
    #[inline]
    pub fn is_mem_overlap(&self, addr: usize, len: usize) -> bool {
        let self_end = self.addr + self.len;
        if let Some(target_end) = addr.checked_add(len) {
            return addr < self_end && self.addr < target_end;
        }
        true
    }

    /// If current mem region contains target region.
    #[inline]
    pub fn is_mem_contained(&self, addr: usize, len: usize) -> bool {
        let self_end = self.addr + self.len;
        if let Some(target_end) = addr.checked_add(len) {
            return addr >= self.addr && target_end <= self_end;
        }
        false
    }
}

pub trait SecMemManager<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    /// Create new manager with no metadata
    fn new() -> Self;
    /// Init new manager
    fn init(&mut self, sm_addr: usize, sm_len: usize) -> bool;
    /// Delete manager, free all resource.
    fn deinit(self, cur_regions: &mut [(usize, usize); MAX_PMP_ENTRY_COUNT as usize]) -> u32;
    /// Create new region with mem.
    ///
    /// # Safety
    ///
    /// The caller must ensure all of the following about the byte range
    /// `addr..addr + len`:
    ///
    /// 1. `addr + len` does not overflow `usize`, so the range does not wrap
    ///    around the address space.
    /// 2. The whole range satisfies the destination requirements of
    ///    [`core::ptr::write_bytes`] applied as
    ///    `write_bytes(addr as *mut u8, 0, len)`: `addr` is non-null, and the
    ///    pointer formed from `addr` is valid for writes of `len` bytes, which
    ///    requires the entire range to lie within a single live allocation
    ///    whose provenance covers it.
    /// 3. The caller owns the range, or holds an equivalent platform
    ///    guarantee that the range contents may be disposed of. NAPOT
    ///    alignment, or any success reported by a [`SecMemProtector`], does
    ///    not establish this guarantee.
    /// 4. The backing memory stays alive and address-stable for as long as
    ///    the manager holds the region: it must not be freed, moved, or
    ///    reused before the manager gives the region back through
    ///    [`SecMemManager::reclaim`] or reports it back while being consumed
    ///    by [`SecMemManager::deinit`].
    /// 5. The caller coordinates all external access to the range and to
    ///    every sub-allocation handed out from it: while the manager holds
    ///    the region, the safe sanitization methods [`SecMemManager::free_em`]
    ///    and [`SecMemManager::deinit`] may write to any part of the range at
    ///    any time, and no concurrent access or still-live external reference
    ///    may exist at such a write.
    /// 6. Once [`SecMemManager::reclaim`] returns a region, the manager no
    ///    longer holds or sanitizes it, and the caller may dispose of the
    ///    memory again. Once [`SecMemManager::deinit`] consumes the manager,
    ///    the manager no longer uses any region it held.
    ///
    /// These are standing obligations of this call, not obligations of the
    /// call's duration: they cover every sanitization write the manager
    /// performs on the registered region while it holds it. The manager
    /// itself writes to the registered range only through its sanitization
    /// methods; writes by allocator implementations it invokes are covered
    /// by the coordination obligation above.
    ///
    /// # Examples
    ///
    /// Registering memory without `unsafe` is rejected at compile time, so
    /// safe code can no longer make the manager sanitize arbitrary
    /// addresses:
    ///
    /// ```compile_fail,E0133
    /// use smm::allocators::NoneAlloc;
    /// use smm::manager::UniSecMemManager;
    /// use smm::SecMemManager;
    ///
    /// let mut manager = UniSecMemManager::<30, NoneAlloc<30>, NoneAlloc<30>>::new();
    /// manager.extend(0x8000_0000, 0x1000);
    /// ```
    unsafe fn extend(&mut self, addr: usize, len: usize) -> bool;
    /// Reclaim an unused allocable region from manager
    fn reclaim(&mut self) -> Option<(usize, usize)>;
    /// Alloc enclave mem from request region of type
    fn alloc_em(&mut self, len: usize, em_type: SecMemType) -> Option<(usize, usize, usize)>;
    /// Free enclave mem back to origin region
    fn free_em(&mut self, addr: usize, len: usize) -> Option<usize>;
    /// Grant access to certain memory area on current hart.
    fn grant_access(&self, addr: usize, len: usize, region_id: usize) -> bool;
    /// Retrive access to certain memory area on current hart.
    fn retrive_access(&self, addr: usize, len: usize, region_id: usize) -> bool;
}

pub trait SecMemProtector {
    /// Alloc a hardware for current region
    fn alloc(&mut self) -> Option<u32>;
    /// Free a hardware
    fn free(&mut self, hwid: u32) -> bool;
    /// Check memory area can be protect by hardware or not.
    ///
    /// This method only reports whether the protecting hardware can cover
    /// the given address range, for example PMP NAPOT alignment and size
    /// constraints. A `true` result does not establish Rust allocation
    /// validity, provenance, writability, or ownership of the range, and no
    /// sanitization write may rely on it as evidence that its destination is
    /// sound. Sanitization soundness is supported only by the
    /// [`SecMemManager::extend`] registration contract and the manager's
    /// region containment checks.
    fn is_protectable(&self, addr: usize, len: usize) -> bool;
    /// Grant access to secure mem on current hart
    fn grant_access(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Retrive access to secure mem on current hart
    fn retrive_access(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Grant access to secure mem on all hart
    fn grant_access_all(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Retrive access to secure mem on all hart
    fn retrive_access_all(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Disable hardware
    fn disable(&self, hwid: u32) -> bool;
    // Enable hardware
    fn enable(&self, hwid: u32) -> bool;
}
