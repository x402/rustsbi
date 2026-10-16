//! Physical bounds of the current firmware image.

use super::{PhysAddr, PhysAddrRange};
use crate::Result;

/// Returns firmware-owned memory, including the stacks established at boot.
pub fn locate_firmware_image() -> Result<PhysAddrRange> {
    let (image_start, image_end) = linker_image_bounds()?;
    let image_end = boot_image_end(image_end);
    PhysAddrRange::new(PhysAddr::new(image_start), PhysAddr::new(image_end))
}

/// The boot-extended image end where the boot flow publishes one; otherwise
/// the linker-supplied bound. Unreachable on non-RISC-V targets, which fail
/// `linker_image_bounds` before this runs.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn boot_image_end(linker_end: usize) -> usize {
    crate::boot::firmware_end().unwrap_or(linker_end)
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn boot_image_end(linker_end: usize) -> usize {
    linker_end
}

pub(crate) fn linker_image_bounds() -> Result<(usize, usize)> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => {
            let (start, end);
            // SAFETY: `sbi_start` and `sbi_end` are addresses supplied by the firmware
            // linker script. `lla` resolves those addresses without dereferencing them.
            unsafe {
                core::arch::asm!(
                    "lla {start}, sbi_start",
                    "lla {end}, sbi_end",
                    start = out(reg) start,
                    end = out(reg) end,
                    options(nomem, nostack),
                );
            }
            Ok((start, end))
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => Err(crate::Error::NotEnoughResources),
    }
}
