//! CoVE PayloadHeader definition and constants.

/// Magic number for CoVE payload header: "COVE" (0x434F5645).
pub const COVE_PAYLOAD_MAGIC: u32 = 0x434F5645;

/// Current version of CoVE payload header.
pub const COVE_PAYLOAD_VERSION: u32 = 1;

/// CoVE Payload Header (4096 bytes / 1 page).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PayloadHeader {
    /// Magic number (0x434F5645 "COVE").
    pub magic: u32,
    /// Header version (1).
    pub version: u32,
    /// TSM binary offset from payload base.
    pub tsm_offset: u64,
    /// TSM binary size in bytes.
    pub tsm_size: u64,
    /// TSM physical load address.
    pub tsm_load_paddr: u64,
    /// TSM physical entry point address.
    pub tsm_entry_paddr: u64,
    /// Host binary offset from payload base.
    pub host_offset: u64,
    /// Host binary size in bytes.
    pub host_size: u64,
    /// Host physical load address.
    pub host_load_paddr: u64,
    /// Host physical entry point address.
    pub host_entry_paddr: u64,
    /// Reserved padding to align to 4096 bytes (4 KiB page).
    pub reserved: [u8; 4024],
}

impl PayloadHeader {
    /// Validate the magic and version of this header.
    pub const fn is_valid(&self) -> bool {
        self.magic == COVE_PAYLOAD_MAGIC && self.version == COVE_PAYLOAD_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn test_payload_header_layout() {
        assert_eq!(size_of::<PayloadHeader>(), 4096);
        assert_eq!(align_of::<PayloadHeader>(), 8);

        assert_eq!(offset_of!(PayloadHeader, magic), 0);
        assert_eq!(offset_of!(PayloadHeader, version), 4);
        assert_eq!(offset_of!(PayloadHeader, tsm_offset), 8);
        assert_eq!(offset_of!(PayloadHeader, tsm_size), 16);
        assert_eq!(offset_of!(PayloadHeader, tsm_load_paddr), 24);
        assert_eq!(offset_of!(PayloadHeader, tsm_entry_paddr), 32);
        assert_eq!(offset_of!(PayloadHeader, host_offset), 40);
        assert_eq!(offset_of!(PayloadHeader, host_size), 48);
        assert_eq!(offset_of!(PayloadHeader, host_load_paddr), 56);
        assert_eq!(offset_of!(PayloadHeader, host_entry_paddr), 64);
        assert_eq!(offset_of!(PayloadHeader, reserved), 72);
    }

    #[test]
    fn test_payload_header_validation() {
        let mut header = PayloadHeader {
            magic: COVE_PAYLOAD_MAGIC,
            version: COVE_PAYLOAD_VERSION,
            tsm_offset: 0x1000,
            tsm_size: 0x400000,
            tsm_load_paddr: 0x80400000,
            tsm_entry_paddr: 0x80400000,
            host_offset: 0x401000,
            host_size: 0x800000,
            host_load_paddr: 0x80800000,
            host_entry_paddr: 0x80800000,
            reserved: [0; 4024],
        };
        assert!(header.is_valid());

        header.magic = 0x12345678;
        assert!(!header.is_valid());

        header.magic = COVE_PAYLOAD_MAGIC;
        header.version = 2;
        assert!(!header.is_valid());
    }
}
