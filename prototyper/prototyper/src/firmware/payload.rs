use core::arch::naked_asm;
use riscv::register::mstatus;

use super::BootInfo;

pub fn get_boot_info(_nonstandard_a2: usize) -> BootInfo {
    let next_address = if crate::sbi::rdsm::is_cove_payload() {
        crate::sbi::rdsm::get_tsm_entry()
    } else {
        get_image_address()
    };
    BootInfo {
        next_address,
        mpp: mstatus::MPP::Supervisor,
    }
}

#[unsafe(naked)]
#[unsafe(link_section = ".payload")]
pub extern "C" fn payload_image() {
    naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_PAYLOAD_PATH"), "\""),)
}

#[inline]
pub fn get_image_address() -> usize {
    payload_image as *const () as usize
}
