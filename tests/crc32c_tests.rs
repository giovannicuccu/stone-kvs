use stone_kvs::wal::crc32c::{crc32c, crc32c_table, crc32c_slice8};

#[test]
fn crc32c_single_byte_returns_known_value() {
    assert_eq!(crc32c(&[0x01]), 0xa016d052);
}

#[test]
fn crc32c_empty_input_returns_zero() {
    assert_eq!(crc32c(&[]), 0x00000000);
}

#[test]
fn crc32c_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c(&[
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64
        ]),
        0xc99465aa
    );
}

// Tests for table-based implementation using same test data
#[test]
fn crc32c_table_single_byte_returns_known_value() {
    assert_eq!(crc32c_table(&[0x01]), 0xa016d052);
}

#[test]
fn crc32c_table_empty_input_returns_zero() {
    assert_eq!(crc32c_table(&[]), 0x00000000);
}

#[test]
fn crc32c_table_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c_table(&[
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64
        ]),
        0xc99465aa
    );
}

// Tests for slice8 implementation using same test data
#[test]
fn crc32c_slice8_single_byte_returns_known_value() {
    assert_eq!(crc32c_slice8(&[0x01]), 0xa016d052);
}

#[test]
fn crc32c_slice8_empty_input_returns_zero() {
    assert_eq!(crc32c_slice8(&[]), 0x00000000);
}

#[test]
fn crc32c_slice8_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c_slice8(&[
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64
        ]),
        0xc99465aa
    );
}

// Cross-validation test to ensure all implementations produce identical results
#[test]
fn crc32c_implementations_produce_identical_results() {
    let test_cases = vec![
        vec![],
        vec![0x01],
        vec![
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64,
        ], // "hello world"
        vec![0x00, 0xff, 0xaa, 0x55],
        vec![0; 100],                   // 100 zeros
        (0..=255).collect::<Vec<u8>>(), // All byte values
        vec![0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0], // 8 bytes
        (0..16).collect::<Vec<u8>>(), // 16 bytes
        (0..17).collect::<Vec<u8>>(), // 17 bytes (not divisible by 8)
    ];

    for test_data in test_cases {
        let bit_result = crc32c(&test_data);
        let table_result = crc32c_table(&test_data);
        let slice8_result = crc32c_slice8(&test_data);
        
        assert_eq!(
            bit_result, table_result,
            "Bit-by-bit and table implementations differ for input: {:?}",
            test_data
        );
        
        assert_eq!(
            table_result, slice8_result,
            "Table and slice8 implementations differ for input: {:?}",
            test_data
        );
    }
}
