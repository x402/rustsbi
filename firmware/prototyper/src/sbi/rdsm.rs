//! RDSM (Root Domain Security Manager) wiring behind the `rdsm` feature.
//!
//! The CoVE extensions mount as ordinary custom extensions on the SBI
//! dispatcher; with the feature off their constructors yield `None`, so
//! feature-off builds keep upstream dispatch behavior bit for bit. The
//! extension IDs below mirror the single sources (the private RDSM contract
//! in cove-sw's `rdsm-abi`, the standard SUPD/COVH/COVI EIDs in
//! `riscv-cove`) and are compile-time asserted against them while the
//! feature is on.
//!
//! Domain switching (TEERET, host→TSM TEECALL) is sequenced end-to-end in
//! `rdsm-policy::switch` on top of the Runtime's declarative execution
//! contexts: the COVH/COVI handlers forward their own extension ID so the
//! TSM sees the original `a7`, and a staged transfer is consumed by the
//! Runtime's ceremony on the same hart's ecall return path.

use runtime::rustsbi::{Extension, SbiRet, VendorSBI};

/// Private RDSM extension: MPT programming and the TEECALL/TEERET machinery.
pub(crate) const EID_RDSM: usize = 0x5244_534D;
/// SUPD discovery extension (single source: `riscv-cove::supd`).
pub(crate) const EID_SUPD: usize = 0x5355_5044;
/// COVH host→TSM forwarding extension (single source: `riscv-cove::host`).
pub(crate) const EID_COVH: usize = 0x434F_5648;
/// COVI interrupt virtualization extension (single source: `riscv-cove::interrupt`).
pub(crate) const EID_COVI: usize = 0x434F_5649;

#[cfg(feature = "rdsm")]
const _: () = {
    assert!(EID_RDSM == rdsm_policy::EID_RDSM);
    assert!(EID_SUPD == rdsm_policy::EID_SUPD);
    assert!(EID_COVH == rdsm_policy::EID_COVH);
    assert!(EID_COVI == rdsm_policy::EID_COVI);
};

// Substrate CSRs the probe may touch; anything else reports unimplemented.
#[cfg(feature = "rdsm")]
const CSR_MSTATEEN0: u16 = 0x30C;
#[cfg(feature = "rdsm")]
const CSR_MMPT: u16 = 0x382;
#[cfg(feature = "rdsm")]
const CSR_MSDCFG: u16 = 0x74E;

/// Publishes the FDT address for the RDSM boot flow (no-op without the feature).
pub(crate) fn set_fdt_address(fdt_address: usize) {
    #[cfg(feature = "rdsm")]
    rdsm_policy::set_fdt_address(fdt_address);
    #[cfg(not(feature = "rdsm"))]
    let _ = fdt_address;
}

/// Whether the embedded payload is a CoVE payload (always false without the feature).
pub(crate) fn is_cove_payload() -> bool {
    #[cfg(feature = "rdsm")]
    {
        return rdsm_policy::is_cove_payload();
    }
    #[cfg(not(feature = "rdsm"))]
    {
        false
    }
}

/// TSM entry physical address recorded from the CoVE payload header.
pub(crate) fn get_tsm_entry() -> usize {
    #[cfg(feature = "rdsm")]
    {
        return rdsm_policy::get_tsm_entry();
    }
    #[cfg(not(feature = "rdsm"))]
    {
        0
    }
}

/// Brings up the Supervisor Domain substrate on the boot hart (no-op
/// without the feature).
///
/// Must run after PMP setup and after this hart's device-tree feature
/// detection; the guarded-CSR probe works before Runtime trap activation
/// (the recovery vector is self-contained, exactly as in `trap::init`'s own
/// Sstc probe).
pub(crate) fn init(hart_id: usize) {
    #[cfg(feature = "rdsm")]
    {
        let mut probe = GuardedCsrProbe;
        let env = rdsm_policy::InitEnv {
            probe: &mut probe,
            payload_base: crate::firmware::image_address(),
            ram_range: crate::platform::main_ram_range(),
            imsic_range: crate::platform::imsic_file_range(),
            hart_id,
            dt_smsdid: crate::sbi::features::hart_has_extension(
                hart_id,
                crate::sbi::features::Extension::Smsdid,
            ),
        };
        rdsm_policy::init(env);
        // Claim the Smmtt MSDEI on Runtime's behalf (acknowledged by
        // clearing the pending SID bits in `msideie`); unclaimed causes
        // keep Runtime's fail-stop default. `msideie` stays 0, so MSDEI
        // never fires until a domain enables it.
        runtime::machine_irq::install(Some(&rdsm_policy::RDSM_MACHINE_IRQ));
        log::info!("RDSM: machine-interrupt policy installed (MSDEI claim path armed)");
    }
    #[cfg(not(feature = "rdsm"))]
    let _ = hart_id;
}

/// The four CoVE extensions bound to their EIDs for the vendor dispatch
/// enum. Without the feature every field stays `None`, so probing reports
/// the extensions as absent and feature-off dispatch keeps upstream
/// behavior bit for bit.
#[derive(VendorSBI)]
#[rustsbi(crate = runtime::rustsbi)]
pub(crate) struct RdsmExtensions {
    #[rustsbi(extension(eid = EID_RDSM))]
    rdsm: Option<RdsmDomain>,
    #[rustsbi(extension(eid = EID_SUPD))]
    supd: Option<SupdDomain>,
    #[rustsbi(extension(eid = EID_COVH))]
    covh: Option<CovhDomain>,
    #[rustsbi(extension(eid = EID_COVI))]
    covi: Option<CoviDomain>,
}

impl RdsmExtensions {
    /// `None` without the `rdsm` feature: the vendor selector then falls
    /// back to platform-specific extensions.
    pub(crate) fn new() -> Option<Self> {
        #[cfg(feature = "rdsm")]
        {
            Some(Self {
                rdsm: Some(RdsmDomain),
                supd: Some(SupdDomain),
                covh: Some(CovhDomain),
                covi: Some(CoviDomain),
            })
        }
        #[cfg(not(feature = "rdsm"))]
        {
            None
        }
    }
}

/// The private RDSM extension.
pub(crate) struct RdsmDomain;

impl Extension for RdsmDomain {
    fn probe(&self) -> usize {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::rdsm_probe();
        }
        #[cfg(not(feature = "rdsm"))]
        {
            0
        }
    }

    fn handle(&self, function: usize, args: [usize; 6]) -> SbiRet {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::handle_rdsm(function, args);
        }
        #[cfg(not(feature = "rdsm"))]
        {
            let _ = (function, args);
            unreachable!("rdsm extensions are never constructed with the feature off")
        }
    }
}

/// The SUPD discovery extension.
pub(crate) struct SupdDomain;

impl Extension for SupdDomain {
    fn probe(&self) -> usize {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::SUPD_PROBE;
        }
        #[cfg(not(feature = "rdsm"))]
        {
            0
        }
    }

    fn handle(&self, function: usize, _args: [usize; 6]) -> SbiRet {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::handle_supd(function);
        }
        #[cfg(not(feature = "rdsm"))]
        {
            let _ = function;
            unreachable!("rdsm extensions are never constructed with the feature off")
        }
    }
}

/// The COVH host→TSM forwarding extension.
pub(crate) struct CovhDomain;

impl Extension for CovhDomain {
    fn probe(&self) -> usize {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::covh_probe();
        }
        #[cfg(not(feature = "rdsm"))]
        {
            0
        }
    }

    fn handle(&self, function: usize, args: [usize; 6]) -> SbiRet {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::handle_teecall(EID_COVH, function, args);
        }
        #[cfg(not(feature = "rdsm"))]
        {
            let _ = (function, args);
            unreachable!("rdsm extensions are never constructed with the feature off")
        }
    }
}

/// The COVI interrupt virtualization extension.
pub(crate) struct CoviDomain;

impl Extension for CoviDomain {
    fn probe(&self) -> usize {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::covi_probe();
        }
        #[cfg(not(feature = "rdsm"))]
        {
            0
        }
    }

    fn handle(&self, function: usize, args: [usize; 6]) -> SbiRet {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::handle_teecall(EID_COVI, function, args);
        }
        #[cfg(not(feature = "rdsm"))]
        {
            let _ = (function, args);
            unreachable!("rdsm extensions are never constructed with the feature off")
        }
    }
}

/// Trap-safe CSR probe built on the Runtime's fallible raw CSR accesses: an
/// access that traps is recovered and reported as unimplemented instead of
/// faulting the firmware.
#[cfg(feature = "rdsm")]
struct GuardedCsrProbe;

#[cfg(feature = "rdsm")]
impl rdsm_policy::rdsm::probe::TrapSafeCsr for GuardedCsrProbe {
    fn read_csr(&self, csr_addr: u16) -> Option<usize> {
        match csr_addr {
            CSR_MSTATEEN0 => runtime::read_csr_raw::<CSR_MSTATEEN0>().ok(),
            CSR_MMPT => runtime::read_csr_raw::<CSR_MMPT>().ok(),
            CSR_MSDCFG => runtime::read_csr_raw::<CSR_MSDCFG>().ok(),
            _ => None,
        }
    }

    fn write_csr(&mut self, csr_addr: u16, value: usize) -> bool {
        match csr_addr {
            // SAFETY: Dispatch owns this hart's trap state, with MIE and
            // MPRV clear and mscratch zero.
            CSR_MSTATEEN0 => unsafe { runtime::write_csr_raw::<CSR_MSTATEEN0>(value) }.is_ok(),
            // SAFETY: Same dispatch context.
            CSR_MMPT => unsafe { runtime::write_csr_raw::<CSR_MMPT>(value) }.is_ok(),
            // SAFETY: Same dispatch context.
            CSR_MSDCFG => unsafe { runtime::write_csr_raw::<CSR_MSDCFG>(value) }.is_ok(),
            _ => false,
        }
    }
}
