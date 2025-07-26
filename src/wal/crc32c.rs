/// Calculate CRC32C checksum for the given data
pub fn crc32c(data: &[u8]) -> u32 {
    const CRC32C_POLYNOMIAL: u32 = 0x82f63b78; // Castagnoli polynomial in reversed form since we process bytes
    //right-to-left
    let mut crc = 0xffffffff; // Initial value
    
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32C_POLYNOMIAL;
            } else {
                crc >>= 1;
            }
        }
    }
    
    !crc // Final XOR
}