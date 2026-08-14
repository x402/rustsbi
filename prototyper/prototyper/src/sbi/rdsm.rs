//! RDSM (Root Domain Security Manager) initialization for the prototyper.
//!
//! This module initializes the Supervisor Domain substrate (Smsdid / Smmpt / Smsdia)
//! during firmware boot, on the boot hart.
//!
//! # Boot Flow Ordering
//!
//! `rdsm_init()` must be called AFTER `firmware::set_pmp()` because:
//!
//! 1. PMP protects M-mode firmware (including RDSM code and data).
//! 2. MPT adds per-SD isolation on top of PMP.
//! 3. The spec requires "MPT and e(PMP) are always active" when Smsdid is
//!    implemented.
//! 4. Access check order: page table → PMP → MPT → all must pass.

use core::cell::Cell;

use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};
use rdsm::domain::SdidAllocator;
use rdsm::interrupt::SidnAllocator;
use rdsm::mpt::MptPageAlloc;

// ── Trap-safe CSR probe ────────────────────────────────────────────────

/// Wrapper around the prototyper's trap-safe CSR access primitives that
/// implements the [`rdsm::probe::TrapSafeCsr`] trait.
///
/// Uses [`Cell`] for interior mutability so that `read_csr(&self)` can
/// mutate the trap-info scratch space.
pub struct PrototyperCsrProbe {
    trap_info: Cell<TrapInfo>,
}

impl PrototyperCsrProbe {
    /// Create a new probe with a default (un-trapped) `TrapInfo`.
    pub fn new() -> Self {
        Self {
            trap_info: Cell::new(TrapInfo::default()),
        }
    }
}

impl Default for PrototyperCsrProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl rdsm::probe::TrapSafeCsr for PrototyperCsrProbe {
    fn read_csr(&self, csr_addr: u16) -> Option<usize> {
        let mut ti = self.trap_info.get();
        // Write `usize::MAX` so that the callee can detect *no trap*.
        ti.mcause = usize::MAX;
        let val = unsafe { csr_read_allow_dyn(csr_addr, &mut ti as *mut TrapInfo) };
        self.trap_info.set(ti);
        if ti.mcause == usize::MAX {
            Some(val)
        } else {
            None
        }
    }

    fn write_csr(&mut self, csr_addr: u16, value: usize) -> bool {
        let mut ti = self.trap_info.get();
        ti.mcause = usize::MAX;
        unsafe { csr_write_allow_dyn(csr_addr, &mut ti as *mut TrapInfo, value) };
        self.trap_info.set(ti);
        ti.mcause == usize::MAX
    }
}

// ── Dynamic CSR dispatch ───────────────────────────────────────────────
//
// `csr_read_allow` / `csr_write_allow` are generic over `const CSR_NUM: u16`
// (compile-time constant).  The `TrapSafeCsr` trait passes the CSR address
// at run time, so we dispatch via a match on the known CSR constants.

/// Dynamic CSR read with trap catching.
///
/// Returns the CSR value.  Check `(*trap_info).mcause` to see whether a
/// trap occurred (`usize::MAX` = no trap, other = trap cause).
///
/// # Safety
///
/// - `trap_info` must point to valid, aligned memory.
/// - The caller must restore `mtvec` after the call (the callee swaps it).
unsafe fn csr_read_allow_dyn(csr: u16, trap_info: *mut TrapInfo) -> usize {
    match csr {
        crate::riscv::csr::CSR_MMPT => unsafe {
            csr_read_allow::<{ crate::riscv::csr::CSR_MMPT }>(trap_info)
        },
        crate::riscv::csr::CSR_MSDCFG => unsafe {
            csr_read_allow::<{ crate::riscv::csr::CSR_MSDCFG }>(trap_info)
        },
        _ => {
            // Unknown / unimplemented CSR — write mcause = 0 (trap occurred).
            unsafe { core::ptr::write_volatile(&mut (*trap_info).mcause, 0) };
            0
        }
    }
}

/// Dynamic CSR write with trap catching.
///
/// Returns nothing.  Check `(*trap_info).mcause` to see whether a trap
/// occurred (`usize::MAX` = no trap, other = trap cause).
///
/// # Safety
///
/// - `trap_info` must point to valid, aligned memory.
/// - The caller must restore `mtvec` after the call (the callee swaps it).
unsafe fn csr_write_allow_dyn(csr: u16, trap_info: *mut TrapInfo, value: usize) {
    match csr {
        crate::riscv::csr::CSR_MMPT => unsafe {
            csr_write_allow::<{ crate::riscv::csr::CSR_MMPT }>(trap_info, value)
        },
        crate::riscv::csr::CSR_MSDCFG => unsafe {
            csr_write_allow::<{ crate::riscv::csr::CSR_MSDCFG }>(trap_info, value)
        },
        _ => {
            // Unknown / unimplemented CSR — write mcause = 0 (trap occurred).
            unsafe { core::ptr::write_volatile(&mut (*trap_info).mcause, 0) };
        }
    }
}

// ── MPT page bump allocator ────────────────────────────────────────────

/// Size of the MPT page pool (256 KiB = 64 pages of 4 KiB).
const MPT_PAGE_POOL_SIZE: usize = 256 * 1024;

/// Static buffer in BSS that backs the MPT bump allocator.
///
/// The linker places this in its own section `.bss.mpt_pool` so that it is
/// zeroed along with the rest of BSS and does not overlap with other data.
#[unsafe(link_section = ".bss.mpt_pool")]
static mut MPT_PAGE_POOL: [u8; MPT_PAGE_POOL_SIZE] = [0; MPT_PAGE_POOL_SIZE];

/// A simple bump-pointer allocator that hands out 4 KiB pages from a
/// contiguous physical memory region.
///
/// Pages are never reclaimed (`free_page` is a no-op).  This is sufficient
/// for Phase 1 where the MPT tree is built once and kept for the firmware
/// lifetime.
struct MptBumpAlloc {
    base_paddr: usize,
    size: usize,
    next_offset: usize,
}

impl MptBumpAlloc {
    /// Create a new bump allocator over the region `[base_paddr, base_paddr + size)`.
    ///
    /// The region must be 4 KiB aligned and reserved (not used by anything else).
    #[allow(dead_code)]
    const fn new(base_paddr: usize, size: usize) -> Self {
        Self {
            base_paddr,
            size,
            next_offset: 0,
        }
    }

    /// Create a bump allocator backed by the static [`MPT_PAGE_POOL`].
    fn from_pool() -> Self {
        let paddr = mpt_page_pool_paddr();
        Self::new(paddr, MPT_PAGE_POOL_SIZE)
    }

    /// Allocate `num_pages` contiguous 4 KiB pages and return the PPN of
    /// the first page, or `None` if the pool is exhausted.
    ///
    /// This advances `next_offset` by `num_pages * 4096` in a single call.
    /// `MptTree::new` achieves the same result through repeated
    /// `alloc_page()` calls (via the `MptPageAlloc` trait); this method
    /// is provided as a more efficient single-call alternative for direct
    /// use outside the trait.
    #[allow(dead_code)]
    fn alloc_contiguous(&mut self, num_pages: usize) -> Option<usize> {
        let bytes = num_pages * 4096;
        if self.next_offset + bytes > self.size {
            return None;
        }
        let ppn = (self.base_paddr + self.next_offset) >> 12;
        self.next_offset += bytes;
        Some(ppn)
    }
}

impl MptPageAlloc for MptBumpAlloc {
    fn alloc_page(&mut self) -> Option<usize> {
        if self.next_offset + 4096 > self.size {
            return None;
        }
        let ppn = (self.base_paddr + self.next_offset) >> 12;
        self.next_offset += 4096;
        Some(ppn)
    }

    fn free_page(&mut self, _ppn: usize) {
        // Bump allocator: no-op.  Pages are not reclaimed in Phase 1.
    }
}

/// Return the physical (runtime) address of the [`MPT_PAGE_POOL`] static.
///
/// Uses `la` to obtain the address.  In M-mode PIE firmware the runtime
/// virtual address equals the physical address after relocation.
fn mpt_page_pool_paddr() -> usize {
    let addr: usize;
    unsafe {
        core::arch::asm!(
            "la {addr}, {sym}",
            addr = out(reg) addr,
            sym = sym MPT_PAGE_POOL,
            options(nomem),
        );
    }
    addr
}

// ── Global domain ID allocators ────────────────────────────────────────

/// Global SDID allocator, initialized once on the boot hart.
///
/// Only the boot hart allocates SDIDs in Phase 1.  Future phases may
/// need synchronization if non-boot harts also allocate.
static mut SDID_ALLOCATOR: SdidAllocator = SdidAllocator::new();

/// Global SIDN allocator, initialized once on the boot hart.
static mut SIDN_ALLOCATOR: SidnAllocator = SidnAllocator::new();

/// Returns a mutable reference to the global SDID allocator.
///
/// # Safety
///
/// Only the boot hart should call this in Phase 1.  Future phases must
/// add synchronization for concurrent access from multiple harts.
#[allow(dead_code)]
pub unsafe fn rdsm_sdid_allocator() -> &'static mut SdidAllocator {
    unsafe { &mut SDID_ALLOCATOR }
}

/// Returns a mutable reference to the global SIDN allocator.
///
/// # Safety
///
/// Only the boot hart should call this in Phase 1.  Future phases must
/// add synchronization for concurrent access from multiple harts.
#[allow(dead_code)]
pub unsafe fn rdsm_sidn_allocator() -> &'static mut SidnAllocator {
    unsafe { &mut SIDN_ALLOCATOR }
}

// ── RDSM initialization ────────────────────────────────────────────────

/// Initialize the Supervisor Domain substrate on the boot hart.
///
/// This function:
///
/// 1. Probes for Smsdid / Smmpt / Smsdia hardware support.
/// 2. Allocates SDID 0 (host supervisor domain) and SIDN 0 (host interrupt
///    domain).
/// 3. Builds an MPT tree with permissive (RWX) permissions covering the
///    platform's main memory range.
/// 4. Activates the host domain by programming the `mmpt` CSR and issuing
///    `MFENCE.PA`.
/// 5. Switches to the host interrupt domain by setting `msdcfg.SIDN = 0`.
///
/// The MPT tree is intentionally leaked — it must persist for the
/// firmware's lifetime.
///
/// # Boot Flow Ordering
///
/// Must be called after `firmware::set_pmp()` on the boot hart.
/// Non-boot harts will have their `mmpt` CSRs left in Bare mode (reset
/// state) and must be programmed with the host MPT root before MPT
/// restrictions are enforced in future phases.
pub fn rdsm_init() {
    use crate::riscv::current_hartid;
    use crate::sbi::features::{Extension, hart_extension_probe};
    use rdsm::{
        domain::activate_domain,
        interrupt::switch_interrupt_domain,
        mpt::{MptPerm, MptTree},
        probe::{probe_sdid_len, probe_smmpt, probe_smsdia, probe_smsdid},
    };

    let hart_id = current_hartid();

    // Check if RDSM extensions are detected via device tree.
    if !hart_extension_probe(hart_id, Extension::Smsdid) {
        info!("RDSM: Smsdid not detected, skipping RDSM initialization");
        return;
    }

    info!("RDSM: Initializing Supervisor Domain substrate");

    // Create trap-safe CSR probe.
    let mut probe = PrototyperCsrProbe::new();

    // Probe hardware support via CSR access.
    if !probe_smsdid(&probe) {
        warn!("RDSM: mmpt CSR not readable, Smsdid not implemented in hardware");
        return;
    }

    let mpt_mode = match probe_smmpt(&mut probe) {
        Some(mode) => mode,
        None => {
            warn!("RDSM: No Smmpt mode supported, skipping MPT initialization");
            return;
        }
    };

    let sdid_len = probe_sdid_len(&mut probe);
    info!(
        "RDSM: Smsdid detected, Smmpt mode={:?}, SDID width={}",
        mpt_mode, sdid_len
    );

    if !probe_smsdia(&probe) {
        warn!("RDSM: Smsdia not implemented, interrupt domain features limited");
    }

    // Allocate SDID 0 for the host supervisor domain using the global
    // allocator so the state persists after rdsm_init returns.
    let host_sdid = unsafe { SDID_ALLOCATOR.alloc() }.expect("SDID allocation for host failed");
    assert_eq!(host_sdid, 0, "Host SDID must be 0");

    // Allocate SIDN 0 for the host interrupt domain using the global
    // allocator so the state persists after rdsm_init returns.
    let host_sidn = unsafe { SIDN_ALLOCATOR.alloc() }.expect("SIDN allocation for host failed");
    assert_eq!(host_sidn, 0, "Host SIDN must be 0");

    // Create MPT page allocator from the reserved pool.
    let mut mpt_alloc = MptBumpAlloc::from_pool();

    // Build the host MPT tree.
    let mut host_tree = match MptTree::new(mpt_mode, &mut mpt_alloc) {
        Some(tree) => tree,
        None => {
            error!("RDSM: Failed to allocate MPT root page");
            return;
        }
    };

    // Set permissive permissions for the platform address range.
    //
    // Phase 1 covers only the platform's memory region to keep page-pool
    // usage bounded.  Full address-space coverage (0 … usize::MAX) requires
    // NAPOT support and is deferred to a future phase.
    let memory_range = match unsafe { crate::platform::PLATFORM.info.memory_range.as_ref() } {
        Some(range) => range.clone(),
        None => {
            error!("RDSM: Platform memory range not initialized");
            return;
        }
    };

    info!(
        "RDSM: Setting MPT permissions for range 0x{:x} – 0x{:x}",
        memory_range.start, memory_range.end
    );

    // Low memory (MMIO, firmware data before main RAM).
    if memory_range.start > 0 {
        host_tree.set_perm(0, memory_range.start, MptPerm::RWX, &mut mpt_alloc);
    }
    // Main platform memory.
    host_tree.set_perm(
        memory_range.start,
        memory_range.end.saturating_sub(memory_range.start),
        MptPerm::RWX,
        &mut mpt_alloc,
    );

    info!(
        "RDSM: Host MPT tree built, root_ppn=0x{:x}, mode={:?}",
        host_tree.root_ppn(),
        mpt_mode
    );

    // Activate the host supervisor domain (writes mmpt CSR + MFENCE.PA).
    activate_domain(host_sdid, &host_tree);

    // Switch to the host interrupt domain (sets msdcfg.SIDN = 0).
    switch_interrupt_domain(host_sidn);

    info!("RDSM: Host supervisor domain (SDID=0) activated, SIDN=0 set");

    // Intentional leak: the MPT tree must persist for the firmware's
    // lifetime.  Dropping it would call `destroy()` and free the pages.
    core::mem::forget(host_tree);

    info!("RDSM: Initialization complete");
}
