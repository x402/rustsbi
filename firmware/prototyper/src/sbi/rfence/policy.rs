//! HFENCE dispatch policy: target preflight and GVMA address encoding.
//!
//! The helpers only depend on `alloc` so the host test entry in
//! `.github/scripts/prototyper-hfence.sh` can unit-test the exact functions
//! the firmware uses, without Runtime, CSR or assembly dependencies.

extern crate alloc;

use alloc::vec::Vec;

/// Collects a snapshot of the fence targets and rejects the whole call when
/// any target lacks the hypervisor extension.
///
/// Returns `Some` with the collected targets in iteration order when every
/// target supports the hypervisor extension, and `None` otherwise. An empty
/// target set succeeds with an empty snapshot. Collecting the snapshot up
/// front keeps the validated set and the dispatched set identical even when
/// hart availability changes between the two phases.
pub(crate) fn preflight_hfence_targets<T>(
    targets: impl IntoIterator<Item = T>,
    supports_h: impl FnMut(&T) -> bool,
) -> Option<Vec<T>> {
    let snapshot: Vec<T> = targets.into_iter().collect();
    if snapshot.iter().all(supports_h) {
        Some(snapshot)
    } else {
        None
    }
}

/// Encodes a byte guest physical address as the `rs1` operand of
/// `hfence.gvma`, which takes the guest physical page number (`GPA >> 2`).
pub(crate) const fn encode_gvma_address(guest_physical_address: usize) -> usize {
    guest_physical_address >> 2
}

#[cfg(test)]
mod tests {
    use super::{encode_gvma_address, preflight_hfence_targets};

    const PAGE_SIZE: usize = 4096;

    #[test]
    fn all_supported_targets_keep_content_and_order() {
        let snapshot = preflight_hfence_targets([5usize, 6, 7], |target| *target != 0)
            .expect("every target supports the hypervisor extension");
        assert_eq!(snapshot, [5, 6, 7]);
    }

    #[test]
    fn empty_target_set_succeeds() {
        let snapshot = preflight_hfence_targets([] as [usize; 0], |_| unreachable!())
            .expect("an empty target set has nothing to reject");
        assert!(snapshot.is_empty());
    }

    #[test]
    fn rejects_when_any_target_lacks_hypervisor() {
        for unsupported in [0usize, 1, 2] {
            let result = preflight_hfence_targets([0usize, 1, 2], |target| *target != unsupported);
            assert_eq!(result, None, "target {unsupported} must fail the preflight");
        }
    }

    #[test]
    fn gvma_encoding_shifts_the_page_number() {
        assert_eq!(encode_gvma_address(0x1000), 0x400);
        assert_eq!(encode_gvma_address(0), 0);
    }

    #[test]
    fn gvma_encoding_covers_each_page_of_a_range() {
        for offset in (0..4 * PAGE_SIZE).step_by(PAGE_SIZE) {
            let page = 0x1000usize.wrapping_add(offset);
            assert_eq!(encode_gvma_address(page), page >> 2);
        }
    }

    #[test]
    fn gvma_encoding_supports_the_highest_page_aligned_address() {
        let highest = usize::MAX & !0xfff;
        assert_eq!(encode_gvma_address(highest), highest >> 2);
    }
}
