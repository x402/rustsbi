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
//! 4. Access check order: page table -> PMP -> MPT -> all must pass.

use core::cell::Cell;

use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};
use rdsm::domain::SdidAllocator;
use rdsm::interrupt::SidnAllocator;
use rdsm::mpt::MptPageAlloc;
pub use rdsm::payload::{COVE_PAYLOAD_MAGIC, COVE_PAYLOAD_VERSION, PayloadHeader};

// ── Private SBI extension constants for RDSM ───────────────────────────

/// Extension ID for private RDSM SBI extension: 0x5244534D ("RDSM").
pub const EID_RDSM: usize = 0x5244534D;

/// Function ID for RDSM_GET_INFO: 0.
pub const FID_RDSM_GET_INFO: usize = 0;

/// Function ID for RDSM_MPT_SET: 1.
pub const FID_RDSM_MPT_SET: usize = 1;

/// Function ID for RDSM_MFENCE_PA: 2.
pub const FID_RDSM_MFENCE_PA: usize = 2;

/// Function ID for RDSM_TEERET: 3.
pub const FID_RDSM_TEERET: usize = 3;

// ── TEERET Reason constants ────────────────────────────────────────────

/// Parameter a0 value for NORMAL_RETURN: 0.
#[allow(dead_code)]
pub const NORMAL_RETURN: usize = 0;

/// Parameter a0 value for TVM_EXIT: 1.
#[allow(dead_code)]
pub const TVM_EXIT: usize = 1;

/// Parameter a0 value for TSM_READY: 2.
#[allow(dead_code)]
pub const TSM_READY: usize = 2;

// ── CoVE Extension ID constants ─────────────────────────────────────────

/// Extension ID for Supervisor Domains Enumeration Extension: 0x53555044 ("SUPD").
pub const EID_SUPD: usize = 0x53555044;

/// Extension ID for CoVE Host Extension: 0x434F5648 ("COVH").
pub const EID_COVH: usize = 0x434F5648;

/// Extension ID for CoVE Interrupt Extension: 0x434F5649 ("COVI").
pub const EID_COVI: usize = 0x434F5649;

// ── THCS (Thread/Hart Context Structure) & DomainContext ────────────────

/// Context of a supervisor domain (Host or TSM/Confidential).
/// Contains general-purpose registers, S-mode CSRs, HS-mode (hypervisor)
/// CSRs, and VS-mode CSRs.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DomainContext {
    // GPRs
    pub ra: usize,
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub t: [usize; 7],
    pub s: [usize; 12],
    pub a: [usize; 8],
    pub pc: usize,

    // S-mode CSRs
    pub sstatus: usize,
    pub stvec: usize,
    pub sip: usize,
    pub sie: usize,
    pub scounteren: usize,
    pub sscratch: usize,
    pub satp: usize,

    // HS-mode (hypervisor) CSRs
    pub hstatus: usize,
    pub hgatp: usize,
    pub hedeleg: usize,
    pub hideleg: usize,
    pub hvip: usize,
    pub henvcfg: usize,
    pub hcounteren: usize,

    // VS-mode CSRs
    pub vsstatus: usize,
    pub vsie: usize,
    pub vstvec: usize,
    pub vsscratch: usize,
    pub vsepc: usize,
    pub vscause: usize,
    pub vstval: usize,
    pub vsip: usize,
    pub vsatp: usize,
}

impl DomainContext {
    pub const fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            gp: 0,
            tp: 0,
            t: [0; 7],
            s: [0; 12],
            a: [0; 8],
            pc: 0,
            sstatus: 0,
            stvec: 0,
            sip: 0,
            sie: 0,
            scounteren: 0,
            sscratch: 0,
            satp: 0,
            hstatus: 0,
            hgatp: 0,
            hedeleg: 0,
            hideleg: 0,
            hvip: 0,
            henvcfg: 0,
            hcounteren: 0,
            vsstatus: 0,
            vsie: 0,
            vstvec: 0,
            vsscratch: 0,
            vsepc: 0,
            vscause: 0,
            vstval: 0,
            vsip: 0,
            vsatp: 0,
        }
    }
}

/// Thread / Hart Context Structure (THCS).
/// Manages domain execution contexts (Host Save State Area: hssa, TSM Save State Area: tssa)
/// and TSM readiness status.
#[derive(Clone, Copy, Debug)]
pub struct Thcs {
    pub hssa: DomainContext,
    pub tssa: DomainContext,
    pub tsm_ready: bool,
}

impl Thcs {
    pub const fn new() -> Self {
        Self {
            hssa: DomainContext::new(),
            tssa: DomainContext::new(),
            tsm_ready: false,
        }
    }
}

pub static mut THCS: Thcs = Thcs::new();

#[inline]
#[allow(dead_code)]
pub fn thcs() -> &'static Thcs {
    unsafe { &THCS }
}

#[inline]
#[allow(dead_code)]
pub unsafe fn thcs_mut() -> &'static mut Thcs {
    unsafe { &mut THCS }
}

#[inline]
#[allow(dead_code)]
pub fn is_tsm_ready() -> bool {
    unsafe { THCS.tsm_ready }
}

/// Save all S-mode, HS-mode and VS-mode CSRs of the current domain into `ctx`.
#[inline(always)]
unsafe fn read_csrs(ctx: &mut DomainContext) {
    #[cfg(target_arch = "riscv64")]
    unsafe {
        // S-mode CSRs
        core::arch::asm!(
            "csrr {sstatus}, sstatus",
            "csrr {stvec}, stvec",
            "csrr {sip}, sip",
            "csrr {sie}, sie",
            "csrr {scounteren}, scounteren",
            "csrr {sscratch}, sscratch",
            "csrr {satp}, satp",
            sstatus = out(reg) ctx.sstatus,
            stvec = out(reg) ctx.stvec,
            sip = out(reg) ctx.sip,
            sie = out(reg) ctx.sie,
            scounteren = out(reg) ctx.scounteren,
            sscratch = out(reg) ctx.sscratch,
            satp = out(reg) ctx.satp,
            options(nomem)
        );
        // HS-mode CSRs
        core::arch::asm!(
            "csrr {hstatus}, 0x600",
            "csrr {hgatp}, 0x680",
            "csrr {hedeleg}, 0x602",
            "csrr {hideleg}, 0x603",
            "csrr {hvip}, 0x645",
            "csrr {henvcfg}, 0x60a",
            "csrr {hcounteren}, 0x606",
            hstatus = out(reg) ctx.hstatus,
            hgatp = out(reg) ctx.hgatp,
            hedeleg = out(reg) ctx.hedeleg,
            hideleg = out(reg) ctx.hideleg,
            hvip = out(reg) ctx.hvip,
            henvcfg = out(reg) ctx.henvcfg,
            hcounteren = out(reg) ctx.hcounteren,
            options(nomem)
        );
        // VS-mode CSRs
        core::arch::asm!(
            "csrr {vsstatus}, 0x200",
            "csrr {vsie}, 0x204",
            "csrr {vstvec}, 0x205",
            "csrr {vsscratch}, 0x240",
            "csrr {vsepc}, 0x241",
            "csrr {vscause}, 0x242",
            "csrr {vstval}, 0x243",
            "csrr {vsip}, 0x244",
            "csrr {vsatp}, 0x280",
            vsstatus = out(reg) ctx.vsstatus,
            vsie = out(reg) ctx.vsie,
            vstvec = out(reg) ctx.vstvec,
            vsscratch = out(reg) ctx.vsscratch,
            vsepc = out(reg) ctx.vsepc,
            vscause = out(reg) ctx.vscause,
            vstval = out(reg) ctx.vstval,
            vsip = out(reg) ctx.vsip,
            vsatp = out(reg) ctx.vsatp,
            options(nomem)
        );
    }
}

/// Restore all S-mode, HS-mode and VS-mode CSRs of a domain from `ctx`.
#[inline(always)]
unsafe fn write_csrs(ctx: &DomainContext) {
    #[cfg(target_arch = "riscv64")]
    unsafe {
        // S-mode CSRs
        core::arch::asm!(
            "csrw sstatus, {sstatus}",
            "csrw stvec, {stvec}",
            "csrw sip, {sip}",
            "csrw sie, {sie}",
            "csrw scounteren, {scounteren}",
            "csrw sscratch, {sscratch}",
            "csrw satp, {satp}",
            sstatus = in(reg) ctx.sstatus,
            stvec = in(reg) ctx.stvec,
            sip = in(reg) ctx.sip,
            sie = in(reg) ctx.sie,
            scounteren = in(reg) ctx.scounteren,
            sscratch = in(reg) ctx.sscratch,
            satp = in(reg) ctx.satp,
            options(nomem)
        );
        // HS-mode CSRs
        core::arch::asm!(
            "csrw 0x600, {hstatus}",
            "csrw 0x680, {hgatp}",
            "csrw 0x602, {hedeleg}",
            "csrw 0x603, {hideleg}",
            "csrw 0x645, {hvip}",
            "csrw 0x60a, {henvcfg}",
            "csrw 0x606, {hcounteren}",
            hstatus = in(reg) ctx.hstatus,
            hgatp = in(reg) ctx.hgatp,
            hedeleg = in(reg) ctx.hedeleg,
            hideleg = in(reg) ctx.hideleg,
            hvip = in(reg) ctx.hvip,
            henvcfg = in(reg) ctx.henvcfg,
            hcounteren = in(reg) ctx.hcounteren,
            options(nomem)
        );
        // VS-mode CSRs
        core::arch::asm!(
            "csrw 0x200, {vsstatus}",
            "csrw 0x204, {vsie}",
            "csrw 0x205, {vstvec}",
            "csrw 0x240, {vsscratch}",
            "csrw 0x241, {vsepc}",
            "csrw 0x242, {vscause}",
            "csrw 0x243, {vstval}",
            "csrw 0x244, {vsip}",
            "csrw 0x280, {vsatp}",
            vsstatus = in(reg) ctx.vsstatus,
            vsie = in(reg) ctx.vsie,
            vstvec = in(reg) ctx.vstvec,
            vsscratch = in(reg) ctx.vsscratch,
            vsepc = in(reg) ctx.vsepc,
            vscause = in(reg) ctx.vscause,
            vstval = in(reg) ctx.vstval,
            vsip = in(reg) ctx.vsip,
            vsatp = in(reg) ctx.vsatp,
            options(nomem)
        );
    }
}

// ── Global RDSM context ────────────────────────────────────────────────

/// Global RDSM context recording dual-MPT state, payload header info, and FDT address.
#[derive(Clone, Copy, Debug)]
pub struct RdsmContext {
    pub is_cove: bool,
    pub mpt_mode: Option<rdsm::csr::MptMode>,
    pub host_sdid: usize,
    pub conf_sdid: usize,
    pub host_sidn: usize,
    pub conf_sidn: usize,
    pub host_root_ppn: usize,
    pub conf_root_ppn: usize,
    pub tsm_load_paddr: usize,
    pub tsm_entry_paddr: usize,
    pub tsm_size: usize,
    pub host_load_paddr: usize,
    pub host_entry_paddr: usize,
    pub host_size: usize,
    pub fdt_address: usize,
}

impl RdsmContext {
    pub const fn new() -> Self {
        Self {
            is_cove: false,
            mpt_mode: None,
            host_sdid: 0,
            conf_sdid: 1,
            host_sidn: 0,
            conf_sidn: 1,
            host_root_ppn: 0,
            conf_root_ppn: 0,
            tsm_load_paddr: 0,
            tsm_entry_paddr: 0,
            tsm_size: 0,
            host_load_paddr: 0,
            host_entry_paddr: 0,
            host_size: 0,
            fdt_address: 0,
        }
    }
}

static mut RDSM_CONTEXT: RdsmContext = RdsmContext::new();

#[inline]
pub fn rdsm_context() -> &'static RdsmContext {
    unsafe { &RDSM_CONTEXT }
}

#[inline]
pub unsafe fn rdsm_context_mut() -> &'static mut RdsmContext {
    unsafe { &mut RDSM_CONTEXT }
}

#[inline]
#[allow(dead_code)]
pub fn is_cove_payload() -> bool {
    unsafe { RDSM_CONTEXT.is_cove }
}

#[inline]
#[allow(dead_code)]
pub fn get_tsm_entry() -> usize {
    unsafe { RDSM_CONTEXT.tsm_entry_paddr }
}

#[inline]
#[allow(dead_code)]
pub fn get_host_entry() -> usize {
    unsafe { RDSM_CONTEXT.host_entry_paddr }
}

#[inline]
#[allow(dead_code)]
pub fn set_fdt_address(fdt: usize) {
    unsafe {
        RDSM_CONTEXT.fdt_address = fdt;
    }
}

#[inline]
#[allow(dead_code)]
pub fn get_fdt_address() -> usize {
    unsafe { RDSM_CONTEXT.fdt_address }
}

// ── CoVE Payload parsing & loading ─────────────────────────────────────

/// Check if payload at `get_image_address()` starts with magic `0x434F5645` ("COVE").
///
/// If yes:
/// - Copy TSM binary from `payload_base + tsm_offset` (length `tsm_size`) to `tsm_load_paddr`.
/// - Copy Host binary from `payload_base + host_offset` (length `host_size`) to `host_load_paddr`.
/// - Record `tsm_entry_paddr` and `host_entry_paddr` in global RDSM context.
///
/// Returns `true` if a valid CoVE payload was loaded, `false` otherwise.
pub fn check_and_load_cove_payload() -> bool {
    let payload_base = crate::firmware::get_image_address();
    if payload_base == 0 {
        return false;
    }

    let magic = unsafe { core::ptr::read_volatile(payload_base as *const u32) };
    if magic != COVE_PAYLOAD_MAGIC {
        return false;
    }

    let header = unsafe { &*(payload_base as *const PayloadHeader) };
    if header.version != COVE_PAYLOAD_VERSION {
        warn!(
            "RDSM: Found CoVE payload magic but unsupported version {}",
            header.version
        );
        return false;
    }

    info!(
        "RDSM: CoVE payload detected (TSM: offset=0x{:x}, size=0x{:x}, load=0x{:x}, entry=0x{:x}; Host: offset=0x{:x}, size=0x{:x}, load=0x{:x}, entry=0x{:x})",
        header.tsm_offset,
        header.tsm_size,
        header.tsm_load_paddr,
        header.tsm_entry_paddr,
        header.host_offset,
        header.host_size,
        header.host_load_paddr,
        header.host_entry_paddr
    );

    // Copy TSM binary
    unsafe {
        let src_tsm = (payload_base as u64 + header.tsm_offset) as *const u8;
        let dst_tsm = header.tsm_load_paddr as *mut u8;
        core::ptr::copy(src_tsm, dst_tsm, header.tsm_size as usize);
    }

    // Copy Host binary
    unsafe {
        let src_host = (payload_base as u64 + header.host_offset) as *const u8;
        let dst_host = header.host_load_paddr as *mut u8;
        core::ptr::copy(src_host, dst_host, header.host_size as usize);
    }

    // Instruction and data memory fences
    #[cfg(target_arch = "riscv64")]
    unsafe {
        core::arch::asm!("fence.i", options(nostack));
        core::arch::asm!("fence rw, rw", options(nostack));
    }

    // Record in global RDSM context
    unsafe {
        let r_ctx = rdsm_context_mut();
        r_ctx.is_cove = true;
        r_ctx.tsm_load_paddr = header.tsm_load_paddr as usize;
        r_ctx.tsm_entry_paddr = header.tsm_entry_paddr as usize;
        r_ctx.tsm_size = header.tsm_size as usize;
        r_ctx.host_load_paddr = header.host_load_paddr as usize;
        r_ctx.host_entry_paddr = header.host_entry_paddr as usize;
        r_ctx.host_size = header.host_size as usize;
    }

    true
}

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
            // Unknown / unimplemented CSR - write mcause = 0 (trap occurred).
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
            // Unknown / unimplemented CSR - write mcause = 0 (trap occurred).
            unsafe { core::ptr::write_volatile(&mut (*trap_info).mcause, 0) };
        }
    }
}

// ── MPT page bump allocator ────────────────────────────────────────────

/// Size of the MPT page pool (4 MiB = 1024 pages of 4 KiB).
const MPT_PAGE_POOL_SIZE: usize = 4 * 1024 * 1024;

/// Base physical address of the MPT page pool in reserved memory (0x8090_0000).
const MPT_PAGE_POOL_PADDR: usize = 0x8090_0000;

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

    /// Create a bump allocator backed by the reserved MPT page pool.
    fn from_pool() -> Self {
        Self::new(MPT_PAGE_POOL_PADDR, MPT_PAGE_POOL_SIZE)
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
/// The MPT tree is intentionally leaked - it must persist for the
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

    // Check and load CoVE payload if present
    let is_cove = check_and_load_cove_payload();

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

    // Allocate SDID 1 for the confidential supervisor domain.
    let conf_sdid = unsafe { SDID_ALLOCATOR.alloc() }.expect("SDID allocation for conf failed");
    assert_eq!(conf_sdid, 1, "Confidential SDID must be 1");

    // Allocate SIDN 0 for the host interrupt domain using the global
    // allocator so the state persists after rdsm_init returns.
    let host_sidn = unsafe { SIDN_ALLOCATOR.alloc() }.expect("SIDN allocation for host failed");
    assert_eq!(host_sidn, 0, "Host SIDN must be 0");

    // Allocate SIDN 1 for the confidential interrupt domain.
    let conf_sidn = unsafe { SIDN_ALLOCATOR.alloc() }.expect("SIDN allocation for conf failed");
    assert_eq!(conf_sidn, 1, "Confidential SIDN must be 1");

    // Create MPT page allocator from the reserved pool.
    let mut mpt_alloc = MptBumpAlloc::from_pool();

    // Build the confidential MPT tree (MPT_CONF, SDID=1).
    let mut conf_tree = match MptTree::new(mpt_mode, &mut mpt_alloc) {
        Some(tree) => tree,
        None => {
            error!("RDSM: Failed to allocate MPT_CONF root page");
            return;
        }
    };

    // Build the host MPT tree (MPT_HOST, SDID=0).
    let mut host_tree = match MptTree::new(mpt_mode, &mut mpt_alloc) {
        Some(tree) => tree,
        None => {
            error!("RDSM: Failed to allocate MPT_HOST root page");
            return;
        }
    };

    // Set permissive permissions for the platform address range.
    //
    // Phase 1 covers only the platform's memory region to keep page-pool
    // usage bounded.  Full address-space coverage (0 ... usize::MAX) requires
    // NAPOT support and is deferred to a future phase.
    let memory_range = match unsafe { crate::platform::PLATFORM.info.memory_range.as_ref() } {
        Some(range) => range.clone(),
        None => {
            error!("RDSM: Platform memory range not initialized");
            return;
        }
    };

    info!(
        "RDSM: Setting MPT permissions for range 0x{:x} - 0x{:x}",
        memory_range.start, memory_range.end
    );

    // Low memory (MMIO, firmware data before main RAM, e.g. 0..0x80000000).
    if memory_range.start > 0 {
        conf_tree.set_perm(0, memory_range.start, MptPerm::RWX, &mut mpt_alloc);
        host_tree.set_perm(0, memory_range.start, MptPerm::RWX, &mut mpt_alloc);
    }

    // MPT_CONF (SDID=1): Full platform RAM range with RWX permissions.
    conf_tree.set_perm(
        memory_range.start,
        memory_range.end.saturating_sub(memory_range.start),
        MptPerm::RWX,
        &mut mpt_alloc,
    );

    // MPT_HOST (SDID=0):
    if is_cove {
        let r_ctx = rdsm_context();
        let tsm_load_paddr = r_ctx.tsm_load_paddr;
        let host_load_paddr = r_ctx.host_load_paddr;

        // Platform RAM before TSM range: RWX
        if tsm_load_paddr > memory_range.start {
            host_tree.set_perm(
                memory_range.start,
                tsm_load_paddr.saturating_sub(memory_range.start),
                MptPerm::RWX,
                &mut mpt_alloc,
            );
        }

        // TSM memory range [tsm_load_paddr, host_load_paddr): NONE (inaccessible)
        if host_load_paddr > tsm_load_paddr {
            host_tree.set_perm(
                tsm_load_paddr,
                host_load_paddr.saturating_sub(tsm_load_paddr),
                MptPerm::NONE,
                &mut mpt_alloc,
            );
        }

        // Platform RAM from host_load_paddr onwards: RWX
        if memory_range.end > host_load_paddr {
            host_tree.set_perm(
                host_load_paddr,
                memory_range.end.saturating_sub(host_load_paddr),
                MptPerm::RWX,
                &mut mpt_alloc,
            );
        }
    } else {
        // Not a CoVE payload: full platform RAM with RWX
        host_tree.set_perm(
            memory_range.start,
            memory_range.end.saturating_sub(memory_range.start),
            MptPerm::RWX,
            &mut mpt_alloc,
        );
    }

    info!(
        "RDSM: MPT trees built: Host root_ppn=0x{:x}, Conf root_ppn=0x{:x}, mode={:?}",
        host_tree.root_ppn(),
        conf_tree.root_ppn(),
        mpt_mode
    );

    // Save both trees/root PPNs and mode globally in RDSM context.
    unsafe {
        let r_ctx = rdsm_context_mut();
        r_ctx.mpt_mode = Some(mpt_mode);
        r_ctx.host_sdid = host_sdid;
        r_ctx.conf_sdid = conf_sdid;
        r_ctx.host_sidn = host_sidn;
        r_ctx.conf_sidn = conf_sidn;
        r_ctx.host_root_ppn = host_tree.root_ppn();
        r_ctx.conf_root_ppn = conf_tree.root_ppn();
    }

    if is_cove {
        println!("[RDSM] Booting...");
        info!("[RDSM] Booting...");

        // Activate Confidential domain (SDID=1, MPT_CONF)
        activate_domain(conf_sdid, &conf_tree);
        switch_interrupt_domain(conf_sidn);
        info!("RDSM: Confidential supervisor domain (SDID=1) activated, SIDN=1 set");
    } else {
        // Activate Host domain (SDID=0, MPT_HOST)
        activate_domain(host_sdid, &host_tree);
        switch_interrupt_domain(host_sidn);
        info!("RDSM: Host supervisor domain (SDID=0) activated, SIDN=0 set");
    }

    // Intentional leak: the MPT trees must persist for the firmware's lifetime.
    core::mem::forget(host_tree);
    core::mem::forget(conf_tree);

    info!("RDSM: Initialization complete");
}

/// Handle COVH / COVI SBI calls (TEECALL) from Host using EntireContext.
///
/// Saves Host context to `THCS.hssa`, switches `mmpt` to SDID=1 (Confidential domain),
/// restores TSM context / S-mode CSRs, passes Host arguments `a0..a7` into `regs.a`,
/// sets `pc = tssa.pc, sp = tssa.sp, gp = tssa.gp, tp = tssa.tp`, and returns `ctx.restore()`.
pub extern "C" fn handle_teecall_entire(ctx: fast_trap::EntireContext) -> fast_trap::EntireResult {
    let (mut ctx, _) = ctx.split();
    let regs = ctx.regs();

    let epc = riscv::register::mepc::read();
    let next_pc = epc + crate::sbi::trap::helper::get_inst(epc).1;
    let sp = riscv::register::mscratch::read();
    let gp = crate::sbi::trap::helper::read_gp();
    let tp = crate::sbi::trap::helper::read_tp();

    let host_a = regs.a;

    // 1. Save Host context to THCS.hssa
    unsafe {
        let th = thcs_mut();
        th.hssa.ra = regs.ra;
        th.hssa.sp = sp;
        th.hssa.gp = gp;
        th.hssa.tp = tp;
        th.hssa.t = regs.t;
        th.hssa.s = regs.s;
        th.hssa.a = host_a;
        th.hssa.pc = next_pc;

        read_csrs(&mut th.hssa);
    }

    // 2. Switch mmpt to SDID=1 (Confidential domain MPT_CONF_ROOT), mfence_pa(0, 0), SIDN=1
    let r_ctx = rdsm_context();
    let mpt_mode = r_ctx.mpt_mode.unwrap_or(rdsm::csr::MptMode::Bare);
    let conf_root_ppn = r_ctx.conf_root_ppn;

    #[cfg(target_arch = "riscv64")]
    {
        let mmpt = rdsm::csr::Mmpt::from_parts(mpt_mode, 1, conf_root_ppn);
        mmpt.write();
        rdsm::fence::mfence_pa(0, 0);
        rdsm::interrupt::switch_interrupt_domain(1);
    }

    // 3. Restore TSM S-mode/HS-mode/VS-mode CSRs & context
    let th = thcs();
    unsafe { write_csrs(&th.tssa) };

    let tsm_pc = th.tssa.pc;
    let tsm_sp = th.tssa.sp;
    let tsm_gp = th.tssa.gp;
    let tsm_tp = th.tssa.tp;

    // Pass Host arguments a0..a7 to TSM
    regs.a = host_a;
    regs.ra = th.tssa.ra;
    regs.t = th.tssa.t;
    regs.s = th.tssa.s;
    regs.pc = tsm_pc;
    regs.sp = tsm_sp;
    regs.gp = tsm_gp;
    regs.tp = tsm_tp;

    crate::sbi::trap::helper::write_gp(tsm_gp);
    crate::sbi::trap::helper::write_tp(tsm_tp);
    unsafe {
        riscv::register::mscratch::write(tsm_sp);
        riscv::register::mepc::write(tsm_pc);
        riscv::register::mstatus::set_mpp(riscv::register::mstatus::MPP::Supervisor);
        riscv::register::mstatus::set_mpie();
    }

    ctx.restore()
}

/// Handle RDSM SBI extension calls (EID 0x5244534D) using EntireContext.
pub extern "C" fn handle_rdsm_entire(ctx: fast_trap::EntireContext) -> fast_trap::EntireResult {
    let (mut ctx, _) = ctx.split();
    let regs = ctx.regs();

    let fid = regs.a[6];
    match fid {
        FID_RDSM_GET_INFO => {
            let r_ctx = rdsm_context();
            let mode_val = r_ctx.mpt_mode.map_or(0, |m| m as usize);
            regs.a[0] = 0; // SBI_SUCCESS
            regs.a[1] = mode_val;
            let epc = riscv::register::mepc::read();
            unsafe {
                riscv::register::mepc::write(epc + crate::sbi::trap::helper::get_inst(epc).1);
            }
            ctx.restore()
        }
        FID_RDSM_MPT_SET => {
            let target_sdid = regs.a[0];
            let paddr = regs.a[1];
            let len = regs.a[2];
            let perm_bits = regs.a[3] as u8;

            let perm = match rdsm::mpt::MptPerm::from_bits(perm_bits) {
                Some(p) => p,
                None => {
                    regs.a[0] = (-3isize) as usize; // SBI_ERR_INVALID_PARAM
                    regs.a[1] = 0;
                    let epc = riscv::register::mepc::read();
                    unsafe {
                        riscv::register::mepc::write(
                            epc + crate::sbi::trap::helper::get_inst(epc).1,
                        );
                    }
                    return ctx.restore();
                }
            };

            let r_ctx = rdsm_context();
            let mode = match r_ctx.mpt_mode {
                Some(m) => m,
                None => {
                    regs.a[0] = (-2isize) as usize; // SBI_ERR_FAILED
                    regs.a[1] = 0;
                    let epc = riscv::register::mepc::read();
                    unsafe {
                        riscv::register::mepc::write(
                            epc + crate::sbi::trap::helper::get_inst(epc).1,
                        );
                    }
                    return ctx.restore();
                }
            };

            let root_ppn = if target_sdid == r_ctx.host_sdid {
                r_ctx.host_root_ppn
            } else if target_sdid == r_ctx.conf_sdid {
                r_ctx.conf_root_ppn
            } else {
                regs.a[0] = (-3isize) as usize; // SBI_ERR_INVALID_PARAM
                regs.a[1] = 0;
                let epc = riscv::register::mepc::read();
                unsafe {
                    riscv::register::mepc::write(epc + crate::sbi::trap::helper::get_inst(epc).1);
                }
                return ctx.restore();
            };

            let mut tree = rdsm::mpt::MptTree::from_root(mode, root_ppn);
            let mut alloc = MptBumpAlloc::from_pool();
            tree.set_perm(paddr, len, perm, &mut alloc);
            core::mem::forget(tree);

            regs.a[0] = 0; // SBI_SUCCESS
            regs.a[1] = 0;
            let epc = riscv::register::mepc::read();
            unsafe {
                riscv::register::mepc::write(epc + crate::sbi::trap::helper::get_inst(epc).1);
            }
            ctx.restore()
        }
        FID_RDSM_MFENCE_PA => {
            let paddr = regs.a[0];
            let sdid = regs.a[1];
            #[cfg(target_arch = "riscv64")]
            rdsm::fence::mfence_pa(paddr, sdid);

            regs.a[0] = 0; // SBI_SUCCESS
            regs.a[1] = 0;
            let epc = riscv::register::mepc::read();
            unsafe {
                riscv::register::mepc::write(epc + crate::sbi::trap::helper::get_inst(epc).1);
            }
            ctx.restore()
        }
        FID_RDSM_TEERET => {
            let reason = regs.a[0];
            match reason {
                TSM_READY => {
                    let a1 = regs.a[1];
                    let r_ctx = rdsm_context();
                    let tsm_entry = if a1 != 0 { a1 } else { r_ctx.tsm_entry_paddr };
                    let sp = riscv::register::mscratch::read();
                    let gp = crate::sbi::trap::helper::read_gp();
                    let tp = crate::sbi::trap::helper::read_tp();

                    unsafe {
                        let th = thcs_mut();
                        th.tssa.pc = tsm_entry;
                        th.tssa.sp = sp;
                        th.tssa.gp = gp;
                        th.tssa.tp = tp;
                        th.tssa.ra = regs.ra;
                        th.tssa.t = regs.t;
                        th.tssa.s = regs.s;
                        th.tssa.a = regs.a;

                        read_csrs(&mut th.tssa);

                        th.tsm_ready = true;
                    }

                    info!("[RDSM] Switching to Host Domain (SDID=0)...");

                    let mpt_mode = r_ctx.mpt_mode.unwrap_or(rdsm::csr::MptMode::Bare);
                    let host_root_ppn = r_ctx.host_root_ppn;

                    #[cfg(target_arch = "riscv64")]
                    {
                        let mmpt = rdsm::csr::Mmpt::from_parts(mpt_mode, 0, host_root_ppn);
                        mmpt.write();
                        rdsm::fence::mfence_pa(0, 0);
                        rdsm::interrupt::switch_interrupt_domain(0);
                    }

                    let host_entry_paddr = r_ctx.host_entry_paddr;
                    let fdt_address = r_ctx.fdt_address;

                    unsafe {
                        if host_entry_paddr & 0x3 == 0 {
                            core::arch::asm!(
                                "csrw stvec, {host_entry_paddr}",
                                host_entry_paddr = in(reg) host_entry_paddr,
                                options(nomem),
                            );
                        }
                        core::arch::asm!("csrw sscratch, zero", "csrw sie, zero", options(nomem));
                        riscv::register::sstatus::clear_sie();
                        riscv::register::satp::write(riscv::register::satp::Satp::from_bits(0));
                        riscv::register::mstatus::set_mpie();
                        riscv::register::mstatus::set_mpp(
                            riscv::register::mstatus::MPP::Supervisor,
                        );
                        riscv::register::mepc::write(host_entry_paddr);
                    }

                    regs.a[0] = crate::riscv::current_hartid();
                    regs.a[1] = fdt_address;
                    regs.pc = host_entry_paddr;

                    ctx.restore()
                }
                NORMAL_RETURN => {
                    let epc = riscv::register::mepc::read();
                    let next_pc = epc + crate::sbi::trap::helper::get_inst(epc).1;
                    let sp = riscv::register::mscratch::read();
                    let gp = crate::sbi::trap::helper::read_gp();
                    let tp = crate::sbi::trap::helper::read_tp();

                    let tsm_err = regs.a[1];
                    let tsm_val = regs.a[2];

                    // 1. Save TSM context to THCS.tssa
                    unsafe {
                        let th = thcs_mut();
                        th.tssa.ra = regs.ra;
                        th.tssa.sp = sp;
                        th.tssa.gp = gp;
                        th.tssa.tp = tp;
                        th.tssa.t = regs.t;
                        th.tssa.s = regs.s;
                        th.tssa.a = regs.a;
                        th.tssa.pc = next_pc;

                        read_csrs(&mut th.tssa);
                    }

                    // 2. Switch mmpt to Host (SDID=0), mfence_pa(0, 0), SIDN=0
                    let r_ctx = rdsm_context();
                    let mpt_mode = r_ctx.mpt_mode.unwrap_or(rdsm::csr::MptMode::Bare);
                    let host_root_ppn = r_ctx.host_root_ppn;

                    #[cfg(target_arch = "riscv64")]
                    {
                        let mmpt = rdsm::csr::Mmpt::from_parts(mpt_mode, 0, host_root_ppn);
                        mmpt.write();
                        rdsm::fence::mfence_pa(0, 0);
                        rdsm::interrupt::switch_interrupt_domain(0);
                    }

                    // 3. Restore Host S-mode/HS-mode/VS-mode CSRs and GPRs
                    let th = thcs();
                    unsafe { write_csrs(&th.hssa) };

                    let host_pc = th.hssa.pc;
                    let host_sp = th.hssa.sp;
                    let host_gp = th.hssa.gp;
                    let host_tp = th.hssa.tp;

                    regs.ra = th.hssa.ra;
                    regs.t = th.hssa.t;
                    regs.s = th.hssa.s;

                    // Write TSM returned error (a1) and value (a2) to Host a0 and a1
                    regs.a[0] = tsm_err;
                    regs.a[1] = tsm_val;
                    regs.a[2] = th.hssa.a[2];
                    regs.a[3] = th.hssa.a[3];
                    regs.a[4] = th.hssa.a[4];
                    regs.a[5] = th.hssa.a[5];
                    regs.a[6] = th.hssa.a[6];
                    regs.a[7] = th.hssa.a[7];

                    regs.gp = host_gp;
                    regs.tp = host_tp;
                    regs.sp = host_sp;
                    regs.pc = host_pc;

                    crate::sbi::trap::helper::write_gp(host_gp);
                    crate::sbi::trap::helper::write_tp(host_tp);
                    unsafe {
                        riscv::register::mscratch::write(host_sp);
                        riscv::register::mepc::write(host_pc);
                        riscv::register::mstatus::set_mpp(
                            riscv::register::mstatus::MPP::Supervisor,
                        );
                        riscv::register::mstatus::set_mpie();
                    }

                    ctx.restore()
                }
                _ => {
                    error!("RDSM: Unsupported TEERET reason: {}", reason);
                    regs.a[0] = (-1isize) as usize; // SBI_ERR_NOT_SUPPORTED
                    regs.a[1] = 0;
                    let epc = riscv::register::mepc::read();
                    unsafe {
                        riscv::register::mepc::write(
                            epc + crate::sbi::trap::helper::get_inst(epc).1,
                        );
                    }
                    ctx.restore()
                }
            }
        }
        _ => {
            regs.a[0] = (-1isize) as usize; // SBI_ERR_NOT_SUPPORTED
            regs.a[1] = 0;
            let epc = riscv::register::mepc::read();
            unsafe {
                riscv::register::mepc::write(epc + crate::sbi::trap::helper::get_inst(epc).1);
            }
            ctx.restore()
        }
    }
}
