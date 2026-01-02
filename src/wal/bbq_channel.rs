/*
    This is an implementation of the BBQ bounded queue described at https://www.usenix.org/conference/atc22/presentation/wang-jiawei
    The idea is to create a channel implementation tailored to the wal needs
*/
use crate::wal::cache_padded::CachePadded;
use std::sync::atomic::{AtomicU64, Ordering};

struct Variable {
    val: CachePadded<AtomicU64>,
}

impl Variable {
    /*
    TO_DO: check che Ordering values in the methods
     */
    fn init() -> Self {
        Variable {
            val: CachePadded::new(AtomicU64::new(0)),
        }
    }

    fn load(&self) -> u64 {
        self.val.load(Ordering::Acquire)
    }

    fn load_and_add(&self, inc: u64) -> u64 {
        self.val.fetch_add(inc, Ordering::AcqRel)
    }

    fn max(&self, max: u64) -> u64 {
        self.val.fetch_max(max, Ordering::AcqRel)
    }
}

fn from_variable_value_to_sub_components(variable_value: u64, offset_size: u32) -> (u64, u64) {
    (
        variable_value >> offset_size,
        variable_value & !(u64::MAX << offset_size),
    )
}

fn to_variable_value_from_sub_components(version: u64, index_offset: u64, offset_size: u32) -> u64 {
    version << offset_size | index_offset & !(u64::MAX << offset_size)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn should_create_variable() {
        let head = Variable::init();
        assert_eq!(0, head.load());
    }

    #[test]
    fn should_add_value_to_variable() {
        let head = Variable::init();
        let value = head.load();
        let increment = 1;
        assert_eq!(value, head.load_and_add(increment));
        assert_eq!(value + increment, head.load());
    }

    #[test]
    fn should_set_max_value_to_variable_when_higher() {
        let head = Variable::init();
        let value = head.load();
        let increment = 1;
        head.load_and_add(increment);
        let max = increment + 2;
        assert_eq!(value + increment, head.max(max));
        assert_eq!(max, head.load());
    }

    #[test]
    fn should_not_set_max_value_to_variable_when_lower() {
        let head = Variable::init();
        let value = head.load();
        let increment = 2;
        head.load_and_add(increment);
        let not_max = increment - 1;
        assert_eq!(value + increment, head.max(not_max));
        assert_eq!(value + increment, head.load());
    }

    #[test]
    fn should_get_zero_value_from_variable_0() {
        let header_value = 0;
        let (version, block_index) = from_variable_value_to_sub_components(header_value, 1);
        assert_eq!(0, version);
        assert_eq!(0, block_index);
    }

    #[test]
    fn should_get_block_index_from_variable_not_zero() {
        let header_value = 4;
        let (version, block_index) = from_variable_value_to_sub_components(header_value, 10);
        assert_eq!(0, version);
        assert_eq!(header_value, block_index);
    }

    #[test]
    fn should_get_block_version_from_variable_not_zero() {
        let offset_size = 10;
        let expected_block_index = 4;
        let expected_version = 2;
        let header_value = expected_block_index | (expected_version << offset_size);
        let (version, block_index) =
            from_variable_value_to_sub_components(header_value, offset_size);
        assert_eq!(expected_version, version);
        assert_eq!(expected_block_index, block_index);
    }

    #[test]
    fn should_get_variable_value_from_zero_components() {
        let offset_size = 10;
        let variable_value = to_variable_value_from_sub_components(0, 0, offset_size);
        assert_eq!(0, variable_value);
    }

    #[test]
    fn should_get_variable_value_from_not_zero_components() {
        let offset_size = 10;
        let version = 2;
        let index_offset = 4;
        let variable_value =
            to_variable_value_from_sub_components(version, index_offset, offset_size);
        assert_eq!(version << offset_size | index_offset, variable_value);
    }
}
