//! 32 字节容器 Header（spec/azodoc-container.md §3）。

pub const MAGIC: [u8; 8] = *b"AZODOC\r\n";
pub const HEADER_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version_major: u8,
    pub version_minor: u8,
    pub header_size: u16,
    pub flags: u32,
    pub zip_offset: u64,
}

impl Default for Header {
    fn default() -> Self {
        Self::v1()
    }
}

impl Header {
    pub fn v1() -> Self {
        Header {
            version_major: 1,
            version_minor: 0,
            header_size: HEADER_SIZE as u16,
            flags: 0,
            zip_offset: HEADER_SIZE as u64,
        }
    }

    /// 从文件头部字节解析字段（不校验 CRC）。
    pub fn parse_fields(data: &[u8]) -> Header {
        Header {
            version_major: data[8],
            version_minor: data[9],
            header_size: u16::from_le_bytes([data[10], data[11]]),
            flags: u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            zip_offset: u64::from_le_bytes([
                data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
            ]),
        }
    }

    /// 前 24 字节的 CRC-32 是否与存储值一致。
    pub fn crc_matches(&self, data: &[u8]) -> bool {
        if data.len() < 28 {
            return false;
        }
        let stored = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        stored == crc32fast::hash(&data[..24])
    }

    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut b = [0u8; HEADER_SIZE];
        b[..8].copy_from_slice(&MAGIC);
        b[8] = self.version_major;
        b[9] = self.version_minor;
        b[10..12].copy_from_slice(&self.header_size.to_le_bytes());
        b[12..16].copy_from_slice(&self.flags.to_le_bytes());
        b[16..24].copy_from_slice(&self.zip_offset.to_le_bytes());
        let crc = crc32fast::hash(&b[..24]);
        b[24..28].copy_from_slice(&crc.to_le_bytes());
        b[28..32].copy_from_slice(&0u32.to_le_bytes());
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_bytes_roundtrip() {
        let h = Header::v1();
        let bytes = h.to_bytes();
        assert_eq!(&bytes[..8], &MAGIC);
        let h2 = Header::parse_fields(&bytes);
        assert_eq!(h, h2);
        assert!(h.crc_matches(&bytes));
    }

    #[test]
    fn crc_detects_corruption() {
        let mut bytes = Header::v1().to_bytes();
        bytes[5] ^= 0xFF; // 破坏 magic 区域（前 24 字节内）
        let h = Header::parse_fields(&bytes);
        assert!(!h.crc_matches(&bytes));
    }
}
