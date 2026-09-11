//! Thin integration shim between the prototyper firmware and the RDSM
//! firmware layer.
//!
//! All RDSM logic lives in the `rdsm-fw` crate (developed in the cove-sw
//! repository together with the TSM). This module only provides what is
//! inherently firmware-specific:
//!
//! - the trap-safe CSR probe built on the prototyper's early-trap
//!   machinery, implementing `rdsm::probe::TrapSafeCsr`;
//! - the `rdsm_init()` adapter that collects the firmware-provided
//!   environment (payload image address, platform RAM range, device-tree
//!   feature gate, trap helper hooks) and hands it to `rdsm_fw::init`;
//! - no-op stubs so the firmware builds unchanged with the `rdsm`
//!   cargo feature disabled (upstream behaviour).
//!
//! With the feature enabled, every public item of `rdsm-fw` is re-exported
//! from here, so callers keep using `crate::sbi::rdsm::…` paths.

// ── Feature-on: re-export the RDSM firmware layer ──────────────────────

#[cfg(feature = "rdsm")]
pub use rdsm_fw::*;

// Substrate modules referenced directly by the trap handlers
// (`rdsm::csr::Mmpt`, `rdsm::fault::MptFault`, `rdsm::interrupt::MsdeiTrap`).
#[cfg(feature = "rdsm")]
pub use rdsm_fw::rdsm::{csr, fault, interrupt};

// ── Feature-on: firmware-side adapter ──────────────────────────────────

#[cfg(feature = "rdsm")]
pub fn rdsm_init() {
    use crate::riscv::current_hartid;
    use crate::sbi::features::{Extension, hart_has_extension};

    let mut probe = PrototyperCsrProbe::new();

    let env = rdsm_fw::InitEnv {
        probe: &mut probe,
        payload_base: crate::firmware::image_address(),
        ram_range: crate::platform::main_ram_range(),
        hart_id: current_hartid(),
        dt_smsdid: hart_has_extension(current_hartid(), Extension::Smsdid),
        hooks: rdsm_fw::RdsmHooks {
            get_inst: crate::sbi::trap::helper::get_inst,
            read_gp: crate::sbi::trap::helper::read_gp,
            write_gp: crate::sbi::trap::helper::write_gp,
            read_tp: crate::sbi::trap::helper::read_tp,
            write_tp: crate::sbi::trap::helper::write_tp,
        },
    };

    rdsm_fw::init(env);
}

/// Wrapper around the prototyper's trap-safe CSR access primitives that
/// implements the [`rdsm::probe::TrapSafeCsr`] trait.
///
/// Uses [`Cell`] for interior mutability so that `read_csr(&self)` can
/// mutate the trap-info scratch space.
#[cfg(feature = "rdsm")]
pub struct PrototyperCsrProbe {
    trap_info: Cell<TrapInfo>,
}

#[cfg(feature = "rdsm")]
impl PrototyperCsrProbe {
    /// Create a new probe with a default (un-trapped) `TrapInfo`.
    pub fn new() -> Self {
        Self {
            trap_info: Cell::new(TrapInfo::default()),
        }
    }
}

#[cfg(feature = "rdsm")]
impl Default for PrototyperCsrProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "rdsm")]
impl rdsm_fw::rdsm::probe::TrapSafeCsr for PrototyperCsrProbe {
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
// `csr_read_allow` / `csr_write_allow` are generic over
// `const CSR_NUM: u16` (compile-time constant). The `TrapSafeCsr`
// trait passes the CSR address at run time, so we dispatch via a
// match on the known CSR constants.

/// Dynamic CSR read with trap catching.
///
/// Returns the CSR value.  Check `(*trap_info).mcause` to see whether a
/// trap occurred (`usize::MAX` = no trap, other = trap cause).
///
/// # Safety
///
/// - `trap_info` must point to valid, aligned memory.
/// - The caller must restore `mtvec` after the call (the callee swaps it).
#[cfg(feature = "rdsm")]
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
/// Returns nothing.  Check `(*trap_info).mcause` to see whether a
/// trap occurred (`usize::MAX` = no trap, other = trap cause).
///
/// # Safety
///
/// - `trap_info` must point to valid, aligned memory.
/// - The caller must restore `mtvec` after the call (the callee swaps it).
#[cfg(feature = "rdsm")]
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

// ── Feature-off: no-op stubs (upstream firmware behaviour) ─────────────

#[cfg(not(feature = "rdsm"))]
pub fn rdsm_init() {}

#[cfg(not(feature = "rdsm"))]
pub fn set_fdt_address(_fdt: usize) {}

/// Only called under the `rdsm` feature (payload.rs); kept so both
/// configurations expose the same call surface.
#[cfg(not(feature = "rdsm"))]
#[allow(dead_code)]
pub fn is_cove_payload() -> bool {
    false
}

/// Only called under the `rdsm` feature (payload.rs); kept so both
/// configurations expose the same call surface.
#[cfg(not(feature = "rdsm"))]
#[allow(dead_code)]
pub fn get_tsm_entry() -> usize {
    0
}

// ── Shared imports ─────────────────────────────────────────────────────

#[cfg(feature = "rdsm")]
use core::cell::Cell;

#[cfg(feature = "rdsm")]
use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};
