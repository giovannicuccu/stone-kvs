## Conclusions

I tried several crc32c implementations for learning/undestanding hwo it works and then how to get optimal performance.
The results are the following ones:
    1. the hardware implementation is the best one
    2. the version that uses 16kb is the best compromise between memory usage and speed, it's twice faster than the 8kb version. The 32kb version is only 20/30% faster than the 16kb version.

So the final implementation will try to use the hardware version falling back to the 16kb version if the cpu does not support it

The code of the 16kb version is deeply inspired by the one created by burntsushi, it's slightly faster and in my opinion it's more elegant:

```rust
pub fn crc32c_16kb(data: &[u8]) -> u32 {
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
```