use stone_kvs::wal::crc32c::{crc32c_bit_by_bit, crc32c_table, crc32c_slice8, crc32c, crc32c_slice32, crc32c_slice16_bt, IncrementalCrc32c, crc32c_sw};

#[test]
fn crc32c_single_byte_returns_known_value() {
    assert_eq!(crc32c_bit_by_bit(&[0x01]), 0xa016d052);
}

#[test]
fn crc32c_empty_input_returns_zero() {
    assert_eq!(crc32c_bit_by_bit(&[]), 0x00000000);
}

#[test]
fn crc32c_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c_bit_by_bit(&[
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

// Tests for slice32 implementation using same test data
#[test]
fn crc32c_slice32_single_byte_returns_known_value() {
    assert_eq!(crc32c_slice32(&[0x01]), 0xa016d052);
}

#[test]
fn crc32c_slice32_empty_input_returns_zero() {
    assert_eq!(crc32c_slice32(&[]), 0x00000000);
}

#[test]
fn crc32c_slice32_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c_slice32(&[
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64
        ]),
        0xc99465aa
    );
}

// Tests for slice16 implementation using same test data
#[test]
fn crc32c_slice16_single_byte_returns_known_value() {
    assert_eq!(crc32c_sw(&[0x01]), 0xa016d052);
}

#[test]
fn crc32c_slice16_empty_input_returns_zero() {
    assert_eq!(crc32c_sw(&[]), 0x00000000);
}

#[test]
fn crc32c_slice16_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c_sw(&[
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64
        ]),
        0xc99465aa
    );
}

#[test]
fn crc32c_slice16_bt_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c_slice16_bt(&[
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
        let bit_result = crc32c_bit_by_bit(&test_data);
        let table_result = crc32c_table(&test_data);
        let slice8_result = crc32c_slice8(&test_data);
        let slice16_result = crc32c_sw(&test_data);
        let slice32_result = crc32c_slice32(&test_data);
        let hw_result = crc32c(&test_data);
        
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
        
        assert_eq!(
            slice8_result, slice16_result,
            "Slice8 and slice16 implementations differ for input: {:?}",
            test_data
        );
        
        assert_eq!(
            slice16_result, slice32_result,
            "Slice16 and slice32 implementations differ for input: {:?}",
            test_data
        );
        
        assert_eq!(
            slice32_result, hw_result,
            "Slice32 and hardware implementations differ for input: {:?}",
            test_data
        );
    }
}

// Tests for hardware implementation - only compiled for supported architectures
#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
#[test]
fn crc32c_hw_single_byte_returns_known_value() {
    assert_eq!(crc32c(&[0x01]), 0xa016d052);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
#[test]
fn crc32c_hw_empty_input_returns_zero() {
    assert_eq!(crc32c(&[]), 0x00000000);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
#[test]
fn crc32c_hw_hello_world_returns_known_value() {
    // Input: "hello world"
    assert_eq!(
        crc32c(&[
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64
        ]),
        0xc99465aa
    );
}

// Tests for incremental CRC32C implementation
#[test]
fn incremental_crc32c_single_update() {
    let data = b"hello world";
    let expected = crc32c_sw(data);

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(data);
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_multiple_updates() {
    let data1 = b"hello ";
    let data2 = b"world";
    let full_data = b"hello world";
    let expected = crc32c_sw(full_data);

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(data1);
    incremental.update(data2);
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_many_small_updates() {
    let data = b"abcdefghijklmnopqrstuvwxyz";
    let expected = crc32c_sw(data);

    let mut incremental = IncrementalCrc32c::new();
    for &byte in data {
        incremental.update(&[byte]);
    }
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_chunk_sizes() {
    let data = vec![0u8; 100];
    let expected = crc32c_sw(&data);

    // Test various chunk sizes
    for chunk_size in [1, 3, 7, 15, 16, 17, 32, 33] {
        let mut incremental = IncrementalCrc32c::new();
        for chunk in data.chunks(chunk_size) {
            incremental.update(chunk);
        }
        let result = incremental.finalize();
        assert_eq!(result, expected, "Failed for chunk size {}", chunk_size);
    }
}

#[test]
fn incremental_crc32c_empty_data() {
    let expected = crc32c_sw(&[]);

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(&[]);
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_value_without_finalize() {
    let data = b"hello world";
    let expected = crc32c_sw(data);

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(data);
    let result = incremental.value();

    assert_eq!(result, expected);

    // Should still be able to use the calculator
    incremental.update(b" again");
    let full_expected = crc32c_sw(b"hello world again");
    assert_eq!(incremental.value(), full_expected);
}

#[test]
fn incremental_crc32c_reset() {
    let data1 = b"hello world";
    let data2 = b"different data";

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(data1);
    let first_result = incremental.value();

    incremental.reset();
    incremental.update(data2);
    let second_result = incremental.finalize();

    assert_eq!(first_result, crc32c_sw(data1));
    assert_eq!(second_result, crc32c_sw(data2));
}

#[test]
fn incremental_crc32c_partial_buffer_fill() {
    // Test with data that doesn't fill the internal 16-byte buffer completely
    let data = b"short";
    let expected = crc32c_sw(data);

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(data);
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_exact_buffer_size() {
    // Test with exactly 16 bytes
    let data = b"exactly16bytesxx";
    assert_eq!(data.len(), 16);
    let expected = crc32c_sw(data);

    let mut incremental = IncrementalCrc32c::new();
    incremental.update(data);
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_large_data() {
    // Test with a large dataset to ensure performance is reasonable
    let data = vec![0x42u8; 10000];
    let expected = crc32c_sw(&data);

    let mut incremental = IncrementalCrc32c::new();
    for chunk in data.chunks(137) { // Prime number chunk size
        incremental.update(chunk);
    }
    let result = incremental.finalize();

    assert_eq!(result, expected);
}

#[test]
fn incremental_crc32c_mixed_buffer_states() {
    // Test various scenarios where the buffer is in different states
    let mut incremental = IncrementalCrc32c::new();

    // Add 5 bytes (buffer partially filled)
    incremental.update(b"hello");

    // Add 3 more bytes (buffer still partially filled)
    incremental.update(b" wo");

    // Add enough to fill and overflow the buffer
    incremental.update(b"rld and more data here");

    let expected = crc32c_sw(b"hello world and more data here");
    assert_eq!(incremental.finalize(), expected);
}

#[test]
fn incremental_crc32c_cross_validation() {
    // Test that incremental implementation matches other implementations
    let test_cases = vec![
        vec![],
        vec![0x01],
        vec![0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64], // "hello world"
        vec![0x00, 0xff, 0xaa, 0x55],
        vec![0; 100], // 100 zeros
        (0..=255).collect::<Vec<u8>>(), // All byte values
        (0..16).collect::<Vec<u8>>(), // 16 bytes
        (0..17).collect::<Vec<u8>>(), // 17 bytes
        (0..33).collect::<Vec<u8>>(), // 33 bytes
    ];

    for test_data in test_cases {
        let expected = crc32c_sw(&test_data);

        // Test single update
        let mut incremental = IncrementalCrc32c::new();
        incremental.update(&test_data);
        assert_eq!(incremental.finalize(), expected, "Single update failed for {:?}", test_data);

        // Test multiple small updates
        let mut incremental = IncrementalCrc32c::new();
        for chunk in test_data.chunks(3) {
            incremental.update(chunk);
        }
        assert_eq!(incremental.finalize(), expected, "Multiple updates failed for {:?}", test_data);
    }
}
