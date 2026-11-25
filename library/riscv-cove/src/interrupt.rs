//! Chapter 11. COVE Interrupt Extension (EID #0x434F5649 "COVI").

/// Extension ID for COVE Interrupt Extension.
#[doc(alias = "SBI_EXT_COVI")]
pub const EID_COVI: usize = crate::eid_from_str("COVI") as _;
pub use fid::*;

/// Parameters for `sbi_covi_init_tvm_aia` (CoVE v0.7 §11.1, Table 33).
///
/// Sits at a non-confidential physical address supplied by the host. The
/// block describes the virtualized IMSIC of the TVM in the TVM's own
/// guest physical address space; it is not the platform's physical
/// interrupt-file array.
///
/// Layout follows the spec's C definition verbatim — `unsigned long`
/// imsic_base_addr followed by five `uint32_t` fields — so the byte image
/// is identical across RV64 implementations (28 bytes padded to 32).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TvmAiaParams {
    /// Base address of the virtualized IMSIC in TVM physical address
    /// space: the address of the IMSIC with group, hart and guest index
    /// all zero (CoVE §11.1).
    pub imsic_base_addr: usize,
    /// Number of group index bits in an IMSIC address.
    pub group_index_bits: u32,
    /// Location of the group index in an IMSIC address. Must be ≥ 24
    /// (CoVE §11.1).
    pub group_index_shift: u32,
    /// Number of hart index bits in an IMSIC address.
    pub hart_index_bits: u32,
    /// Number of guest index bits in an IMSIC address. Must be at least
    /// log2(guests_per_hart + 1) (CoVE §11.1).
    pub guest_index_bits: u32,
    /// Guest interrupt files per vCPU-hart. Implementations may reject
    /// values > 0 when nested IMSIC virtualization is unsupported.
    pub guests_per_hart: u32,
}

const _: () = assert!(core::mem::size_of::<TvmAiaParams>() == 32);
const _: () = assert!(core::mem::align_of::<TvmAiaParams>() == 8);

#[cfg(test)]
mod layout_tests {
    use super::TvmAiaParams;
    use core::mem::offset_of;

    #[test]
    fn tvm_aia_params_layout_matches_spec() {
        assert_eq!(offset_of!(TvmAiaParams, imsic_base_addr), 0);
        assert_eq!(offset_of!(TvmAiaParams, group_index_bits), 8);
        assert_eq!(offset_of!(TvmAiaParams, group_index_shift), 12);
        assert_eq!(offset_of!(TvmAiaParams, hart_index_bits), 16);
        assert_eq!(offset_of!(TvmAiaParams, guest_index_bits), 20);
        assert_eq!(offset_of!(TvmAiaParams, guests_per_hart), 24);
        assert_eq!(core::mem::size_of::<TvmAiaParams>(), 32);
    }
}

/// Declared in §11.
mod fid {
    /// Function ID to initialize TVM AIA.
    ///
    /// Declared in §11.1.
    #[doc(alias = "SBI_EXT_COVI_INIT_TVM_AIA")]
    pub const INIT_TVM_AIA: usize = 0;
    /// Function ID to set the guest physical address of the specified vCPU's virtualized IMSIC.
    ///
    /// Declared in §11.2.
    #[doc(alias = "SBI_EXT_COVI_SET_TVM_AIA_CPU_IMSIC_ADDR")]
    pub const SET_TVM_AIA_CPU_IMSIC_ADDR: usize = 1;
    /// Function ID to convert the non-confidential guest interrupt file for use with a TVM.
    ///
    /// Declared in §11.3.
    #[doc(alias = "SBI_EXT_COVI_CONVERT_AIA_IMSIC")]
    pub const CONVERT_AIA_IMSIC: usize = 2;
    /// Function ID to reclaim the confidential TVM interrupt file.
    ///
    /// Declared in §11.4.
    #[doc(alias = "SBI_EXT_COVI_RECLAIM_TVM_AIA_IMSIC")]
    pub const RECLAIM_TVM_AIA_IMSIC: usize = 3;
    /// Function ID to bind a TVM vCPU to the current physical CPU.
    ///
    /// Declared in §11.5.
    #[doc(alias = "SBI_EXT_COVI_BIND_AIA_IMSIC")]
    pub const BIND_AIA_IMSIC: usize = 4;
    /// Function ID to begin the unbinding process for the specified vCPU from its guest interrupt files.
    ///
    /// Declared in §11.6.
    #[doc(alias = "SBI_EXT_COVI_UNBIND_AIA_IMSIC_BEGIN")]
    pub const UNBIND_AIA_IMSIC_BEGIN: usize = 5;
    /// Function ID to complete the unbinding process for the specified vCPU from its guest interrupt files.
    ///
    /// Declared in §11.7.
    #[doc(alias = "SBI_EXT_COVI_UNBIND_AIA_IMSIC_END")]
    pub const UNBIND_AIA_IMSIC_END: usize = 6;
    /// Function ID to inject an external interrupt into the specified vCPU.
    ///
    /// Declared in §11.8.
    #[doc(alias = "SBI_EXT_COVI_INJECT_TVM_CPU")]
    pub const INJECT_TVM_CPU: usize = 7;
    /// Function ID to begin the rebinding process for the specified vCPU to the current physical CPU.
    ///
    /// Declared in §11.9.
    #[doc(alias = "SBI_EXT_COVI_REBIND_AIA_IMSIC_BEGIN")]
    pub const REBIND_AIA_IMSIC_BEGIN: usize = 8;
    /// Function ID to clone the old guest interrupt file of the specified vCPU.
    ///
    /// Declared in §11.10.
    #[doc(alias = "SBI_EXT_COVI_REBIND_AIA_IMSIC_CLONE")]
    pub const REBIND_AIA_IMSIC_CLONE: usize = 9;
    /// Function ID to complete the rebinding process for the specified vCPU.
    ///
    /// Declared in §11.11.
    #[doc(alias = "SBI_EXT_COVI_REBIND_AIA_IMSIC_END")]
    pub const REBIND_AIA_IMSIC_END: usize = 10;
}
