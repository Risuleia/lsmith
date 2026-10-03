const POLYNOMIAL: u32 = 0xEDB8_8320;

const TABLE: [u32; 256] = generate_table();

const fn generate_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;

        while j < 8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ POLYNOMIAL } else { crc >> 1 };

            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

#[inline]
pub fn checksum(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF;

    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ TABLE[index]
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(checksum(b""), 0x0000_0000);
    }

    #[test]
    fn standard_test_vector() {
        assert_eq!(checksum(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn hello_world() {
        assert_eq!(checksum(b"hello world"), 0x0D4A_1185);
    }

    #[test]
    fn deterministic() {
        let data = b"lsmith";

        assert_eq!(checksum(data), checksum(data));
    }

    #[test]
    fn different_data() {
        assert_ne!(checksum(b"hello"), checksum(b"world"));
    }

    #[test]
    fn large_input() {
        let data = vec![0xAB; 1024 * 1024];

        let first = checksum(&data);
        let second = checksum(&data);

        assert_eq!(first, second);
    }
}
