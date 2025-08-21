// CRC32C (Castagnoli) polynomial in reversed form since we process bytes right-to-left
const CRC32C_POLYNOMIAL: u32 = 0x82f63b78;

/// Generate CRC32C lookup table at compile time using const fn
const fn generate_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;

        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32C_POLYNOMIAL;
            } else {
                crc >>= 1;
            }
            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

// Pre-computed CRC32C lookup table generated at compile time
const CRC32C_TABLE: [u32; 256] = generate_crc32c_table();

/// Calculate CRC32C checksum for the given data
///
/// Here is my understanding of crc32 in general:
/// At the theory level, we must divide the input by the polynomial (CRC32C_POLYNOMIAL)
/// and then take the remainder. It's not a 'normal' division but a specific one
/// its implementation is simple:
/// 1. Take the initial value
/// 2. Pad it with a number of zeros equal to the number of bits in the polynomial
/// 3. XOR the first n (n=polynomial length) bits of the number with the polynomial (it's a 'special' number predetermined by the standard)
/// 4. Shift by one the number (including the zero bits), XOR with the next n bits and so on
/// 5. The result is the CRC32 value
///
/// The concrete implementation is a little different:
/// the theory says that you should start with bits on the left and process them left-to-right
/// the theory says that you should add zeros on the right
/// the theory says that you should shift the results of poly^(n-input-bits)
/// in practice you shift the input right, in order to do so you need to reverse the polynomial
/// i.e. swap the order of the bits the first one becomes the last and the last one becomes the first and so on
/// there is no need to add zeros on the left because if you shift the input when you exceed the input length the shifted input
/// is filled with zeros
pub fn crc32c(data: &[u8]) -> u32 {
    //right-to-left
    let mut crc = 0xffffffff;

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

    !crc
}

/// Table-based CRC32C implementation that processes one byte at a time
/// this approch works because of the current property
/// CRC(VAL1 XOR VAL2) = CRC(VAL1) XOR CRC(VAL2)
/// VAL1 is the initial value with the first 24 bit from left zeroed
/// VAL2 is the initial value with the last 8 bit from left zeroed
/// CRC(VAL1) is calculated using the table
/// CRC(VAL2) is simply VAL2>>8 since the last 8 bits are zero and the
/// algorithm in this case simply requires a shift
pub fn crc32c_table(data: &[u8]) -> u32 {
    let mut crc = 0xffffffff;
    for &byte in data {
        crc = (crc >> 8) ^ CRC32C_TABLE[((crc as u8) ^ byte) as usize];
    }

    !crc
}

/// Generate 8 CRC32C lookup tables for slicing-by-8 implementation
const fn generate_crc32c_tables_8() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    
    // First table is the standard CRC32C table
    tables[0] = generate_crc32c_table();
    
    // Generate remaining 7 tables
    let mut table_idx = 1;
    while table_idx < 8 {
        let mut i = 0;
        while i < 256 {
            let mut crc = tables[table_idx - 1][i];
            crc = tables[0][(crc & 0xff) as usize] ^ (crc >> 8);
            tables[table_idx][i] = crc;
            i += 1;
        }
        table_idx += 1;
    }
    
    tables
}

// Pre-computed 8 CRC32C lookup tables for slicing-by-8
const CRC32C_TABLES_8: [[u32; 256]; 8] = generate_crc32c_tables_8();

/// CRC32C implementation using slicing-by-8 technique
/// Processes 8 bytes at a time using 8 lookup tables for better performance
pub fn crc32c_slice8(data: &[u8]) -> u32 {
    let mut crc = 0xffffffff;
    let mut i = 0;
    
    // Process 8 bytes at a time
    while i + 8 <= data.len() {
        // Read 8 bytes as u64 (little-endian)
        let chunk = [
            data[i], data[i + 1], data[i + 2], data[i + 3],
            data[i + 4], data[i + 5], data[i + 6], data[i + 7]
        ];
        
        // XOR the first 4 bytes with current CRC
        let crc_bytes = [
            (crc & 0xff) as u8,
            ((crc >> 8) & 0xff) as u8,
            ((crc >> 16) & 0xff) as u8,
            ((crc >> 24) & 0xff) as u8,
        ];
        
        // Calculate new CRC using all 8 tables
        crc = CRC32C_TABLES_8[7][(chunk[0] ^ crc_bytes[0]) as usize]
            ^ CRC32C_TABLES_8[6][(chunk[1] ^ crc_bytes[1]) as usize]
            ^ CRC32C_TABLES_8[5][(chunk[2] ^ crc_bytes[2]) as usize]
            ^ CRC32C_TABLES_8[4][(chunk[3] ^ crc_bytes[3]) as usize]
            ^ CRC32C_TABLES_8[3][chunk[4] as usize]
            ^ CRC32C_TABLES_8[2][chunk[5] as usize]
            ^ CRC32C_TABLES_8[1][chunk[6] as usize]
            ^ CRC32C_TABLES_8[0][chunk[7] as usize];
            
        i += 8;
    }
    
    // Process remaining bytes using single-byte method
    while i < data.len() {
        crc = (crc >> 8) ^ CRC32C_TABLE[((crc as u8) ^ data[i]) as usize];
        i += 1;
    }
    
    !crc
}
