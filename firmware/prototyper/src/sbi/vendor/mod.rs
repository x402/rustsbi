//! Vendor-specific SBI extension selection.

pub(crate) mod allwinner;

use runtime::{
    Result,
    memory::{MemoryRegistry, SupervisorMemory},
    rustsbi::{self, VendorSBI},
};

use crate::platform::allwinner::v821::V821;

/// Vendor-extension devices selected for this platform.
pub(crate) struct Extension {
    v821: Option<allwinner::v821::Extension>,
}

impl Extension {
    pub(crate) fn bind(v821: Option<V821>, memory: &mut MemoryRegistry) -> Result<Self> {
        let v821 = v821
            .map(|platform| allwinner::v821::Extension::bind(platform, memory))
            .transpose()?;
        Ok(Self { v821 })
    }
}

/// Vendor-specific extension set selected for the current platform.
#[derive(VendorSBI)]
#[rustsbi(crate = rustsbi)]
pub(crate) enum Vendor {
    Allwinner(allwinner::v821::Extensions),
    Rdsm(crate::sbi::rdsm::RdsmExtensions),
}

impl Vendor {
    pub(crate) fn new(extension: Extension, memory: &'static SupervisorMemory) -> Option<Self> {
        // The CoVE extensions take precedence while the `rdsm` feature is
        // on; the selector dispatches by matching the constructed variant,
        // and the feature only ships on CoVE targets without platform
        // vendor extensions.
        if let Some(extensions) = crate::sbi::rdsm::RdsmExtensions::new() {
            return Some(Self::Rdsm(extensions));
        }
        extension
            .v821
            .map(|extension| Self::Allwinner(extension.into_extensions(memory)))
    }
}
