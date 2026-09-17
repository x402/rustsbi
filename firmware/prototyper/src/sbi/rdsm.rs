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
//! `rdsm-policy::switch` but cannot be reached from custom extension
//! dispatch yet: it needs Runtime trap-frame access, proposed upstream as
//! rustsbi#286. The live paths reject with their private error codes until
//! that lands.

use runtime::rustsbi::{Extension, SbiRet};

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
            hart_id,
            dt_smsdid: crate::sbi::features::hart_has_extension(
                hart_id,
                crate::sbi::features::Extension::Smsdid,
            ),
        };
        rdsm_policy::init(env);
    }
    #[cfg(not(feature = "rdsm"))]
    let _ = hart_id;
}

/// The private RDSM extension.
pub(crate) struct RdsmDomain;

impl RdsmDomain {
    pub(crate) fn new() -> Option<Self> {
        #[cfg(feature = "rdsm")]
        {
            Some(Self)
        }
        #[cfg(not(feature = "rdsm"))]
        {
            None
        }
    }
}

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

impl SupdDomain {
    pub(crate) fn new() -> Option<Self> {
        #[cfg(feature = "rdsm")]
        {
            Some(Self)
        }
        #[cfg(not(feature = "rdsm"))]
        {
            None
        }
    }
}

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

impl CovhDomain {
    pub(crate) fn new() -> Option<Self> {
        #[cfg(feature = "rdsm")]
        {
            Some(Self)
        }
        #[cfg(not(feature = "rdsm"))]
        {
            None
        }
    }
}

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
            return rdsm_policy::handle_teecall(function, args);
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

impl CoviDomain {
    pub(crate) fn new() -> Option<Self> {
        #[cfg(feature = "rdsm")]
        {
            Some(Self)
        }
        #[cfg(not(feature = "rdsm"))]
        {
            None
        }
    }
}

impl Extension for CoviDomain {
    fn probe(&self) -> usize {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::COVI_PROBE;
        }
        #[cfg(not(feature = "rdsm"))]
        {
            0
        }
    }

    fn handle(&self, function: usize, args: [usize; 6]) -> SbiRet {
        #[cfg(feature = "rdsm")]
        {
            return rdsm_policy::handle_teecall(function, args);
        }
        #[cfg(not(feature = "rdsm"))]
        {
            let _ = (function, args);
            unreachable!("rdsm extensions are never constructed with the feature off")
        }
    }
}

/// Trap-safe CSR probe built on the Runtime's guarded CSR primitives: an
/// access that traps is recovered and reported as unimplemented instead of
/// faulting the firmware.
#[cfg(feature = "rdsm")]
struct GuardedCsrProbe;

#[cfg(feature = "rdsm")]
impl rdsm_policy::rdsm::probe::TrapSafeCsr for GuardedCsrProbe {
    fn read_csr(&self, csr_addr: u16) -> Option<usize> {
        match csr_addr {
            CSR_MSTATEEN0 => runtime::trap::read_csr_guarded::<CSR_MSTATEEN0>().ok(),
            CSR_MMPT => runtime::trap::read_csr_guarded::<CSR_MMPT>().ok(),
            CSR_MSDCFG => runtime::trap::read_csr_guarded::<CSR_MSDCFG>().ok(),
            _ => None,
        }
    }

    fn write_csr(&mut self, csr_addr: u16, value: usize) -> bool {
        match csr_addr {
            CSR_MSTATEEN0 => runtime::trap::write_csr_guarded::<CSR_MSTATEEN0>(value).is_ok(),
            CSR_MMPT => runtime::trap::write_csr_guarded::<CSR_MMPT>(value).is_ok(),
            CSR_MSDCFG => runtime::trap::write_csr_guarded::<CSR_MSDCFG>(value).is_ok(),
            _ => false,
        }
    }
}
