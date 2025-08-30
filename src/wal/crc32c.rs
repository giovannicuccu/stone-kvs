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

// Pre-computed CRC32C lookup table generated at compile time
const CRC32C_TABLE: [u32; 256] = generate_crc32c_table();

const CRC32C_TABLES_8: [[u32; 256]; 8] = generate_crc32c_tables_8();

/// Generate 32 CRC32C lookup tables for slicing-by-32 implementation
const fn generate_crc32c_tables_32() -> [[u32; 256]; 32] {
    let mut tables = [[0u32; 256]; 32];

    // First table is the standard CRC32C table
    tables[0] = generate_crc32c_table();

    // Generate remaining 31 tables
    let mut table_idx = 1;
    while table_idx < 32 {
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

const CRC32C_TABLES_32: [[u32; 256]; 32] = generate_crc32c_tables_32();

/// Generate 16 CRC32C lookup tables for slicing-by-16 implementation
const fn generate_crc32c_tables_16() -> [[u32; 256]; 16] {
    let mut tables = [[0u32; 256]; 16];

    // First table is the standard CRC32C table
    tables[0] = generate_crc32c_table();

    // Generate remaining 15 tables
    let mut table_idx = 1;
    while table_idx < 16 {
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

const CRC32C_TABLES_16: [[u32; 256]; 16] = generate_crc32c_tables_16();

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

// Pre-computed 8 CRC32C lookup tables for slicing-by-8


/// CRC32C implementation using slicing-by-8 technique
/// Processes 8 bytes at a time using 8 lookup tables for better performance
pub fn crc32c_slice8(data: &[u8]) -> u32 {
    let mut crc = 0xffffffff;
    
    let chunks = data.chunks_exact(8);
    let remainder = chunks.remainder();
    
    for chunk in chunks {
        let [b0, b1, b2, b3, b4, b5, b6, b7] = *chunk else {
            unreachable!("chunks_exact(8) guarantees 8 bytes")
        };
        crc ^=  u32::from_le_bytes([b0,b1,b2,b3]);
        crc = CRC32C_TABLES_8[0][b7 as usize]
            ^ CRC32C_TABLES_8[1][b6 as usize]
            ^ CRC32C_TABLES_8[2][b5 as usize]
            ^ CRC32C_TABLES_8[3][b4 as usize]
            ^ CRC32C_TABLES_8[4][(crc >> 24) as u8 as usize]
            ^ CRC32C_TABLES_8[5][(crc >> 16)  as u8 as usize]
            ^ CRC32C_TABLES_8[6][(crc >> 8) as u8 as usize]
            ^ CRC32C_TABLES_8[7][crc as u8 as usize]
        ;
    }
    
    for &byte in remainder {
        crc = (crc >> 8) ^ CRC32C_TABLE[((crc as u8) ^ byte) as usize];
    }
    
    !crc
}

/// Hardware-accelerated CRC32C implementation using CPU intrinsics
/// Falls back to table-based implementation if hardware support is not available
pub fn crc32c_hw(data: &[u8]) -> u32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("sse4.2") {
            return crc32c_hw_x86(data);
        }
    }
    
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("crc") {
            return crc32c_hw_arm(data);
        }
    }
    
    // Fallback to table-based implementation
    crc32c_table(data)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn crc32c_hw_x86(data: &[u8]) -> u32 {
    use std::arch::x86_64::*;
    
    unsafe {
        let mut crc = 0xffffffffu32;
        
        let (prefix, u64s, suffix) = data.align_to::<u64>();
        
        // Process unaligned prefix bytes
        for &byte in prefix {
            crc = _mm_crc32_u8(crc, byte);
        }
        
        // Process aligned u64 chunks
        for &value in u64s {
            crc = _mm_crc32_u64(crc as u64, value) as u32;
        }
        
        // Process remaining suffix bytes
        for &byte in suffix {
            crc = _mm_crc32_u8(crc, byte);
        }
        
        !crc
    }
}

#[cfg(target_arch = "aarch64")]
fn crc32c_hw_arm(data: &[u8]) -> u32 {
    use std::arch::aarch64::*;
    
    unsafe {
        let mut crc = 0xffffffffu32;
        
        let (prefix, u64s, suffix) = data.align_to::<u64>();
        
        // Process unaligned prefix bytes
        for &byte in prefix {
            crc = __crc32cb(crc, byte);
        }
        
        // Process aligned u64 chunks
        for &value in u64s {
            crc = __crc32cd(crc, value);
        }
        
        // Process remaining suffix bytes
        for &byte in suffix {
            crc = __crc32cb(crc, byte);
        }
        
        !crc
    }
}

/// CRC32C implementation using slicing-by-32 technique
/// Processes 32 bytes at a time using 32 lookup tables for maximum performance
pub fn crc32c_slice32(data: &[u8]) -> u32 {
    let mut crc = 0xffffffff;
    
    let chunks = data.chunks_exact(32);
    let remainder = chunks.remainder();
    
    for chunk in chunks {
        let [
            b0, b1, b2, b3, b4, b5, b6, b7,
            b8, b9, b10, b11, b12, b13, b14, b15,
            b16, b17, b18, b19, b20, b21, b22, b23,
            b24, b25, b26, b27, b28, b29, b30, b31
        ] = *chunk else {
            unreachable!("chunks_exact(32) guarantees 32 bytes")
        };

        crc ^=  u32::from_le_bytes([b0,b1,b2,b3]);
        crc = CRC32C_TABLES_32[0][b31 as usize]
            ^ CRC32C_TABLES_32[1][b30 as usize]
            ^ CRC32C_TABLES_32[2][b29 as usize]
            ^ CRC32C_TABLES_32[3][b28 as usize]
            ^ CRC32C_TABLES_32[4][b27 as usize]
            ^ CRC32C_TABLES_32[5][b26 as usize]
            ^ CRC32C_TABLES_32[6][b25 as usize]
            ^ CRC32C_TABLES_32[7][b24 as usize]
            ^ CRC32C_TABLES_32[8][b23 as usize]
            ^ CRC32C_TABLES_32[9][b22 as usize]
            ^ CRC32C_TABLES_32[10][b21 as usize]
            ^ CRC32C_TABLES_32[11][b20 as usize]
            ^ CRC32C_TABLES_32[12][b19 as usize]
            ^ CRC32C_TABLES_32[13][b18 as usize]
            ^ CRC32C_TABLES_32[14][b17 as usize]
            ^ CRC32C_TABLES_32[15][b16 as usize]
            ^ CRC32C_TABLES_32[16][b15 as usize]
            ^ CRC32C_TABLES_32[17][b14 as usize]
            ^ CRC32C_TABLES_32[18][b13 as usize]
            ^ CRC32C_TABLES_32[19][b12 as usize]
            ^ CRC32C_TABLES_32[20][b11 as usize]
            ^ CRC32C_TABLES_32[21][b10 as usize]
            ^ CRC32C_TABLES_32[22][b9 as usize]
            ^ CRC32C_TABLES_32[23][b8 as usize]
            ^ CRC32C_TABLES_32[24][b7 as usize]
            ^ CRC32C_TABLES_32[25][b6 as usize]
            ^ CRC32C_TABLES_32[26][b5 as usize]
            ^ CRC32C_TABLES_32[27][b4 as usize]
            ^ CRC32C_TABLES_32[28][(crc >> 24) as u8 as usize]
            ^ CRC32C_TABLES_32[29][(crc >> 16) as u8 as usize]
            ^ CRC32C_TABLES_32[30][(crc >> 8) as u8 as usize]
            ^ CRC32C_TABLES_32[31][ crc  as u8 as usize]
        ;
    }
    
    // Process remaining bytes using table-based approach
    for &byte in remainder {
        crc = (crc >> 8) ^ CRC32C_TABLE[((crc as u8) ^ byte) as usize];
    }
    
    !crc
}

pub fn read_u32_le(slice: &[u8]) -> u32 {
    u32::from_le_bytes(slice[..4].try_into().unwrap())
}

pub fn crc32c_slice16_bt(mut buf: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    while buf.len() >= 16 {
        crc ^= read_u32_le(buf);
        crc = CRC32C_TABLES_16[0][buf[15] as usize]
            ^ CRC32C_TABLES_16[1][buf[14] as usize]
            ^ CRC32C_TABLES_16[2][buf[13] as usize]
            ^ CRC32C_TABLES_16[3][buf[12] as usize]
            ^ CRC32C_TABLES_16[4][buf[11] as usize]
            ^ CRC32C_TABLES_16[5][buf[10] as usize]
            ^ CRC32C_TABLES_16[6][buf[9] as usize]
            ^ CRC32C_TABLES_16[7][buf[8] as usize]
            ^ CRC32C_TABLES_16[8][buf[7] as usize]
            ^ CRC32C_TABLES_16[9][buf[6] as usize]
            ^ CRC32C_TABLES_16[10][buf[5] as usize]
            ^ CRC32C_TABLES_16[11][buf[4] as usize]
            ^ CRC32C_TABLES_16[12][(crc >> 24) as u8 as usize]
            ^ CRC32C_TABLES_16[13][(crc >> 16) as u8 as usize]
            ^ CRC32C_TABLES_16[14][(crc >> 8) as u8 as usize]
            ^ CRC32C_TABLES_16[15][(crc) as u8 as usize];
        buf = &buf[16..];
    }
    for &b in buf {
        crc = CRC32C_TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}

/// CRC32C implementation using slicing-by-16 technique
/// Processes 16 bytes at a time using 16 lookup tables for high performance
pub fn crc32c_slice16(data: &[u8]) -> u32 {
    let mut crc = 0xffffffff;

    let (chunks, remainder) = data.as_chunks::<16>();

    for &[ b0, b1, b2, b3, b4, b5, b6, b7,
           b8, b9, b10, b11, b12, b13, b14, b15] in chunks {
        crc ^=  u32::from_le_bytes([b0,b1,b2,b3]);
        crc = CRC32C_TABLES_16[0][b15 as usize]
            ^ CRC32C_TABLES_16[1][b14 as usize]
            ^ CRC32C_TABLES_16[2][b13 as usize]
            ^ CRC32C_TABLES_16[3][b12 as usize]
            ^ CRC32C_TABLES_16[4][b11 as usize]
            ^ CRC32C_TABLES_16[5][b10 as usize]
            ^ CRC32C_TABLES_16[6][b9 as usize]
            ^ CRC32C_TABLES_16[7][b8 as usize]
            ^ CRC32C_TABLES_16[8][b7 as usize]
            ^ CRC32C_TABLES_16[9][b6 as usize]
            ^ CRC32C_TABLES_16[10][b5 as usize]
            ^ CRC32C_TABLES_16[11][b4 as usize]
            ^ CRC32C_TABLES_16[12][(crc >> 24) as u8 as usize]
            ^ CRC32C_TABLES_16[13][(crc >> 16) as u8 as usize]
            ^ CRC32C_TABLES_16[14][(crc >> 8) as u8 as usize]
            ^ CRC32C_TABLES_16[15][ crc  as u8 as usize]
        ;
    }

    // Process remaining bytes using table-based approach
    for &byte in remainder {
        crc = (crc >> 8) ^ CRC32C_TABLE[((crc as u8) ^ byte) as usize];
    }

    !crc
}