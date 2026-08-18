//! Chapter 9. Supervisor Domains Enumeration Extension (EID #0x53555044 "SUPD").
//!
//! This common extension enumerates capabilities for supervisor domains such
//! as number of active supervisor domains and capabilities of each supervisor
//! domain, e.g., used for CoVE.

/// Extension ID for Supervisor Domains Enumeration Extension.
#[doc(alias = "SBI_EXT_SUPD")]
pub const EID_SUPD: usize = crate::eid_from_str("SUPD") as _;
pub use fid::*;

/// Declared in §9.
mod fid {
    /// Function ID to enumerate active supervisor domains.
    ///
    /// Returns a 64-bit vector with bits set for supervisor domains that are
    /// active. Default value is 1 since supervisor domain 0 is always required
    /// (the hosting domain).
    ///
    /// Declared in §9.1.
    #[doc(alias = "SBI_EXT_SUPD_GET_ACTIVE_DOMAINS")]
    pub const GET_ACTIVE_DOMAINS: usize = 0;
}
