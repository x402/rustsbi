use riscv::register::mstatus;

/// The next stage is the embedded payload image, entered in S-mode.
///
/// With a CoVE payload the TSM is the first supervisor stage; it hands
/// control to the host domain through the RDSM TEERET flow.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (mstatus::MPP, usize) {
    #[cfg(feature = "rdsm")]
    if crate::sbi::rdsm::is_cove_payload() {
        return (mstatus::MPP::Supervisor, crate::sbi::rdsm::get_tsm_entry());
    }
    (mstatus::MPP::Supervisor, payload_address())
}

/// Physical address of the embedded payload image (CoVE payload loading).
#[cfg(feature = "rdsm")]
pub(crate) fn image_address() -> usize {
    payload_image.address().as_usize()
}

#[inline]
fn payload_address() -> usize {
    payload_image.address().as_usize()
}

include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
include!(concat!(env!("OUT_DIR"), "/generated_payload.rs"));
#[cfg(feature = "fdt")]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
