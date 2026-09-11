use riscv::register::mstatus;

/// The next stage is the embedded payload image, entered in S-mode.
///
/// With the `rdsm` feature, a valid CoVE payload image redirects the next
/// stage to the TSM entry point recorded by `rdsm_init()`.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (mstatus::MPP, usize) {
    #[cfg(feature = "rdsm")]
    if crate::sbi::rdsm::is_cove_payload() {
        return (mstatus::MPP::Supervisor, crate::sbi::rdsm::get_tsm_entry());
    }
    (mstatus::MPP::Supervisor, payload_address())
}

/// Address of the embedded payload image (the RDSM payload parser reads
/// its CoVE header from here).
#[cfg_attr(not(feature = "rdsm"), allow(dead_code))]
#[inline]
pub fn image_address() -> usize {
    payload_address()
}

#[inline]
fn payload_address() -> usize {
    payload_image.address().as_usize()
}

include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
include!(concat!(env!("OUT_DIR"), "/generated_payload.rs"));
#[cfg(feature = "fdt")]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
