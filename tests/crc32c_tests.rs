use stone_kvs::wal::crc32c::{crc32c, crc32c_table};

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

// Cross-validation test to ensure both implementations produce identical results
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
    ];

    for test_data in test_cases {
        assert_eq!(
            crc32c(&test_data),
            crc32c_table(&test_data),
            "Implementations differ for input: {:?}",
            test_data
        );
    }
}
