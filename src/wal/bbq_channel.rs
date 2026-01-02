/*
    This is an implementation of the BBQ bounded queue described at https://www.usenix.org/conference/atc22/presentation/wang-jiawei
    The idea is to create a channel implementation tailored to the wal needs
*/
use crate::wal::cache_padded::CachePadded;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};

struct Variable {
    val: CachePadded<AtomicU64>,
}

impl Variable {
    /*
    TO_DO: check che Ordering values in the methods
     */
    fn init(initial_value: u64) -> Self {
        Variable {
            val: CachePadded::new(AtomicU64::new(initial_value)),
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

fn from_variable_value_to_sub_components(variable_value: u64, offset_bits_size: u32) -> (u64, u64) {
    (
        variable_value >> offset_bits_size,
        variable_value & !(u64::MAX << offset_bits_size),
    )
}

fn to_variable_value_from_sub_components(
    version: u64,
    index_offset: u64,
    offset_bits_size: u32,
) -> u64 {
    version << offset_bits_size | index_offset & !(u64::MAX << offset_bits_size)
}

#[derive(Debug, PartialEq)]
enum BlockState {
    Done,
    Allocated(u64),
}

struct Block<T> {
    pub(crate) allocated: Variable,
    pub(crate) committed: Variable,

    pub(crate) reserved: Variable,
    pub(crate) consumed: Variable,

    block_size: usize,
    offset_bits_size: u32,

    entries: *mut MaybeUninit<T>,
}

unsafe impl<T: Send> Send for Block<T> {}
unsafe impl<T: Sync> Sync for Block<T> {}

impl<T> Block<T> {
    fn init(block_size: usize, initial_offset: u64, offset_bits_size: u32) -> Self {
        let entries_slice = Box::<[T]>::new_uninit_slice(block_size);
        let entries = Box::into_raw(entries_slice) as *mut MaybeUninit<T>;
        Block {
            allocated: Variable::init(initial_offset),
            committed: Variable::init(initial_offset),
            reserved: Variable::init(initial_offset),
            consumed: Variable::init(initial_offset),
            block_size,
            offset_bits_size,
            entries,
        }
    }

    fn allocate_entry(&self) -> BlockState {
        let (_, offset) =
            from_variable_value_to_sub_components(self.allocated.load(), self.offset_bits_size);
        if (offset as usize) >= self.block_size {
            return BlockState::Done;
        }
        let (_, offset) = from_variable_value_to_sub_components(
            self.allocated.load_and_add(1),
            self.offset_bits_size,
        );
        if (offset as usize) >= self.block_size {
            return BlockState::Done;
        }
        BlockState::Allocated(offset)
    }
}

impl<T> Drop for Block<T> {
    fn drop(&mut self) {
        let (consumed_version, consumed_offset) =
            from_variable_value_to_sub_components(self.consumed.load(), self.offset_bits_size);
        let (committed_version, committed_offset) =
            from_variable_value_to_sub_components(self.committed.load(), self.offset_bits_size);

        // Sanity check: version should never differ by more than 1
        debug_assert!(
            committed_version <= consumed_version + 1,
            "Block versions differ by more than 1: consumed={}, committed={}",
            consumed_version,
            committed_version
        );

        let start_offset = if consumed_version < committed_version {
            0
        } else {
            consumed_offset
        };

        for i in start_offset as usize..std::cmp::min(committed_offset as usize, self.block_size) {
            unsafe {
                self.entries.add(i).drop_in_place();
            }
        }

        unsafe {
            _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                self.entries,
                self.block_size,
            ));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use env_logger::init;

    #[test]
    fn should_create_variable() {
        let initial_value: u64 = 0;
        let head = Variable::init(initial_value);
        assert_eq!(initial_value, head.load());
    }

    #[test]
    fn should_add_value_to_variable() {
        let initial_value: u64 = 0;
        let head = Variable::init(initial_value);
        let value = head.load();
        let increment = 1;
        assert_eq!(value, head.load_and_add(increment));
        assert_eq!(value + increment, head.load());
    }

    #[test]
    fn should_set_max_value_to_variable_when_higher() {
        let initial_value: u64 = 0;
        let head = Variable::init(initial_value);
        let value = head.load();
        let increment = 1;
        head.load_and_add(increment);
        let max = increment + 2;
        assert_eq!(value + increment, head.max(max));
        assert_eq!(max, head.load());
    }

    #[test]
    fn should_not_set_max_value_to_variable_when_lower() {
        let initial_value: u64 = 0;
        let head = Variable::init(initial_value);
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

    #[test]
    fn should_create_a_block_with_zero_offset() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        assert_eq!(initial_offset, block.allocated.load());
        assert_eq!(initial_offset, block.committed.load());
        assert_eq!(initial_offset, block.reserved.load());
        assert_eq!(initial_offset, block.consumed.load());
    }

    #[test]
    fn should_create_a_block_with_not_zero_offset() {
        let initial_offset = 1;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        assert_eq!(initial_offset, block.allocated.load());
        assert_eq!(initial_offset, block.committed.load());
        assert_eq!(initial_offset, block.reserved.load());
        assert_eq!(initial_offset, block.consumed.load());
    }

    #[test]
    fn should_allocate_entry_return_done_when_allocated_is_greater_than_block_size() {
        let initial_offset = 1;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        block.allocated.load_and_add(block_size as u64 + 1);
        let block_state = block.allocate_entry();
        assert_eq!(BlockState::Done, block_state);
    }

    #[test]
    fn should_allocate_entry_return_done_when_allocated_is_equal_to_block_size() {
        let initial_offset = 1;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        block.allocated.load_and_add(block_size as u64);
        let block_state = block.allocate_entry();
        assert_eq!(BlockState::Done, block_state);
    }

    #[test]
    fn should_allocate_entry_return_allocated_when_allocated_is_lesser_than_block_size() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        block.allocated.load_and_add(block_size as u64 - 1);
        let block_state = block.allocate_entry();
        assert_eq!(BlockState::Allocated(block_size as u64 - 1), block_state);
    }
}
