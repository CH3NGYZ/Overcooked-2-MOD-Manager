use super::*;

#[test]
fn reads_fixed_pe_file_version_without_windows_apis() {
    let mut bytes = vec![0_u8; 80];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&64_u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes.extend([0xbd, 0x04, 0xef, 0xfe, 0x00, 0x00, 0x01, 0x00]);
    bytes.extend(((5_u32 << 16) | 4).to_le_bytes());
    bytes.extend(((23_u32 << 16) | 5).to_le_bytes());
    assert_eq!(portable_pe_version(&bytes).as_deref(), Some("5.4.23.5"));
    assert!(portable_pe_version(&bytes[..88]).is_none());
    bytes[64] = 0;
    assert!(portable_pe_version(&bytes).is_none());
    assert!(portable_pe_version(b"ELF fixture").is_none());
}
