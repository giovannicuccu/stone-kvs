use stone_kvs::wal::crc32c::crc32c;

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
    assert_eq!(crc32c(&[0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x77, 0x6f, 0x72, 0x6c, 0x64]), 0xc99465aa);
}