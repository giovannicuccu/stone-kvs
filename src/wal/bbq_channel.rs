/*
    This is an implementation of the BBQ bounded queue described at https://www.usenix.org/conference/atc22/presentation/wang-jiawei
    The idea is to create a channel implementation tailored to the wal needs
*/
use crate::wal::cache_padded::CachePadded;
use std::mem::{ManuallyDrop, MaybeUninit};
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

#[inline]
fn from_variable_value_to_sub_components(variable_value: u64, offset_bits_size: u32) -> (u64, u64) {
    (
        variable_value >> offset_bits_size,
        variable_value & !(u64::MAX << offset_bits_size),
    )
}

#[inline]
fn to_variable_value_from_sub_components(
    version: u64,
    index_offset: u64,
    offset_bits_size: u32,
) -> u64 {
    version << offset_bits_size | index_offset & !(u64::MAX << offset_bits_size)
}

#[derive(Debug, PartialEq)]
enum BlockState {
    Done(u64),
    Allocated(u64),
    NoEntry(),
    NotAvailable(),
    Reserved(u64),
}

struct Block<T> {
    pub(crate) allocated: Variable,
    pub(crate) committed: Variable,

    pub(crate) reserved: Variable,
    pub(crate) consumed: Variable,

    block_size: usize,
    pub(crate) offset_bits_size: u32,

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
        let (version, offset) = self.to_sub_components(self.allocated.load());
        if (offset as usize) >= self.block_size {
            return BlockState::Done(version);
        }
        let (version, offset) = self.to_sub_components(self.allocated.load_and_add(1));
        if (offset as usize) >= self.block_size {
            return BlockState::Done(version);
        }
        BlockState::Allocated(offset)
    }

    fn commit_entry(&self, offset: u64, data: T) {
        unsafe {
            let data_ptr = self.entries.add(offset as usize);
            data_ptr.write(MaybeUninit::new(data));
        }
        self.committed.load_and_add(1);
    }

    fn reserve_entry(&self) -> BlockState {
        loop {
            let reserved_value = self.reserved.load();
            println!(
                "reserve_entry reserved_value={:?}, offset_bits_size={}",
                reserved_value, self.offset_bits_size
            );
            let (version, reserved_offset) = self.to_sub_components(reserved_value);
            if (reserved_offset as usize) >= self.block_size {
                return BlockState::Done(version);
            }
            let (_, committed_offset) = self.to_sub_components(self.committed.load());
            if reserved_offset == committed_offset {
                return BlockState::NoEntry();
            }
            if (committed_offset as usize) != self.block_size {
                let (_, allocated_offset) = self.to_sub_components(self.allocated.load());
                if allocated_offset != committed_offset {
                    return BlockState::NotAvailable();
                }
            }
            if self.reserved.max(reserved_value + 1) == reserved_value {
                return BlockState::Reserved(reserved_offset);
            }
        }
    }

    fn consume_entry(&self, offset: u64) -> T {
        let data = unsafe { self.entries.add(offset as usize).read().assume_init() };
        self.consumed.load_and_add(1);
        data
    }

    #[inline]
    fn to_sub_components(&self, variable_value: u64) -> (u64, u64) {
        from_variable_value_to_sub_components(variable_value, self.offset_bits_size)
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

#[derive(Debug, PartialEq)]
enum EnqueueState<T> {
    Ok(),
    Full(T),
}

#[derive(Debug, PartialEq)]
enum DequeueState<T> {
    Ok(T),
    Empty(),
}

#[derive(Debug, PartialEq)]
enum AdvanceProducerState {
    Success(),
    NoEntry(),
    NotAvailable(),
}

struct Queue<T> {
    producer_head: Variable,
    consumer_head: Variable,
    blocks: *mut Block<T>,
    block_size: usize,
    blocks_num: usize,
    header_offset_bits_size: u32,
}

const DEFAULT_BLOCK_SIZE: usize = 4;
impl<T> Queue<T> {
    fn with_capacity(capacity: usize) -> Self {
        let blocks_num = capacity / DEFAULT_BLOCK_SIZE + 1;
        let header_offset_bits_size = u64::BITS - blocks_num.leading_zeros();

        let block_offset_bits_size = u64::BITS - DEFAULT_BLOCK_SIZE.leading_zeros() + 1;

        let mut blocks = Vec::with_capacity(blocks_num);
        blocks.push(Block::init(
            DEFAULT_BLOCK_SIZE,
            0u64,
            block_offset_bits_size,
        ));
        for _ in 1..blocks_num {
            blocks.push(Block::init(
                DEFAULT_BLOCK_SIZE,
                DEFAULT_BLOCK_SIZE as u64,
                block_offset_bits_size,
            ));
        }
        unsafe { blocks.set_len(blocks_num) }

        let blocks_ptr = ManuallyDrop::new(blocks).as_mut_ptr();

        Self {
            producer_head: Variable::init(0),
            consumer_head: Variable::init(0),
            block_size: DEFAULT_BLOCK_SIZE,
            blocks_num,
            blocks: blocks_ptr,
            header_offset_bits_size,
        }
    }

    fn enqueue(&self, data: T) -> EnqueueState<T> {
        loop {
            let (producer_head_value, block) = unsafe {
                let (val, ptr) = self.get_producer_head_value_and_block();
                (val, &mut *ptr)
            };
            let block_state = block.allocate_entry();
            match block_state {
                BlockState::Allocated(offset) => {
                    println!("block allocated offset={}", offset);
                    block.commit_entry(offset, data);
                    println!("block committed");
                    return EnqueueState::Ok();
                }
                BlockState::Done(version) => {
                    let advance_producer_state = self.advance_producer_head(producer_head_value);
                    println!("producer advanced state={:?}", advance_producer_state);
                    match advance_producer_state {
                        AdvanceProducerState::Success() => {}
                        AdvanceProducerState::NoEntry() => {
                            return EnqueueState::Full(data);
                        }
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn get_producer_head_value_and_block(&self) -> (u64, *mut Block<T>) {
        let producer_head_value = self.producer_head.load();
        let (_, index) = from_variable_value_to_sub_components(
            producer_head_value,
            self.header_offset_bits_size,
        );
        unsafe { (producer_head_value, self.blocks.add(index as usize)) }
    }

    fn advance_producer_head(&self, producer_head_value: u64) -> AdvanceProducerState {
        let (version, index) = from_variable_value_to_sub_components(
            producer_head_value,
            self.header_offset_bits_size,
        );
        unsafe {
            println!(
                "Advance producer_head_value={}, new index={}",
                producer_head_value,
                (index + 1) as usize % self.blocks_num
            );
            let new_block = self.blocks.add((index + 1) as usize % self.blocks_num);
            let consumed_value = (*new_block).consumed.load();
            let (consumed_version, consumed_offset) = from_variable_value_to_sub_components(
                consumed_value,
                (*new_block).offset_bits_size,
            );
            if consumed_version < version
                || (consumed_version == version && consumed_offset != self.block_size as u64)
            {
                let reserved_value = (*new_block).reserved.load();
                let (_, reserved_offset) = from_variable_value_to_sub_components(
                    reserved_value,
                    (*new_block).offset_bits_size,
                );
                return if reserved_offset == consumed_offset {
                    AdvanceProducerState::NoEntry()
                } else {
                    AdvanceProducerState::NotAvailable()
                };
            }
            let new_variable_value = to_variable_value_from_sub_components(
                version + 1,
                0,
                (*new_block).offset_bits_size,
            );
            (*new_block).committed.max(new_variable_value);
            (*new_block).allocated.max(new_variable_value);
            self.producer_head.max(producer_head_value + 1);
        }
        AdvanceProducerState::Success()
    }

    fn dequeue(&self) -> DequeueState<T> {
        loop {
            let (consumer_head_value, block) = unsafe {
                let (val, ptr) = self.get_consumer_head_value_and_block();
                (val, &mut *ptr)
            };
            let block_state = block.reserve_entry();
            match block_state {
                BlockState::Reserved(offset) => {
                    let data = block.consume_entry(offset);
                    return DequeueState::Ok(data);
                }
                BlockState::Done(version) => {
                    if !self.advance_consumer_head(consumer_head_value) {
                        return DequeueState::Empty();
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn advance_consumer_head(&self, consumer_head_value: u64) -> bool {
        let (version, index) = from_variable_value_to_sub_components(
            consumer_head_value,
            self.header_offset_bits_size,
        );
        println!(
            "Advance consumer_head_value={}, new index={}",
            consumer_head_value,
            (index + 1) as usize % self.blocks_num
        );
        unsafe {
            let new_block = self.blocks.add((index + 1) as usize % self.blocks_num);
            let committed_value = (*new_block).committed.load();
            let (committed_version, committed_offset) = from_variable_value_to_sub_components(
                committed_value,
                (*new_block).offset_bits_size,
            );
            let new_variable_value = to_variable_value_from_sub_components(
                version + 1,
                0,
                (*new_block).offset_bits_size,
            );
            if committed_version != version + 1 {
                return false;
            }
            (*new_block).consumed.max(new_variable_value);
            (*new_block).reserved.max(new_variable_value);
            self.consumer_head.max(consumer_head_value + 1);
            true
        }
    }

    fn get_consumer_head_value_and_block(&self) -> (u64, *mut Block<T>) {
        let consumer_head_value = self.consumer_head.load();
        let (_, index) = from_variable_value_to_sub_components(
            consumer_head_value,
            self.header_offset_bits_size,
        );
        println!(
            "consumer_head_value={}, current index={}",
            consumer_head_value, index
        );
        unsafe { (consumer_head_value, self.blocks.add(index as usize)) }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        println!("Queue drop");
        unsafe {
            Vec::from_raw_parts(self.blocks, self.blocks_num, self.blocks_num);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
        assert_eq!(BlockState::Done(0), block_state);
    }

    #[test]
    fn should_allocate_entry_return_done_when_allocated_is_equal_to_block_size() {
        let initial_offset = 1;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        block.allocated.load_and_add(block_size as u64);
        let block_state = block.allocate_entry();
        assert_eq!(BlockState::Done(0), block_state);
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

    #[test]
    fn should_commit_an_allocated_entry() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        block.allocated.load_and_add(1);
        let committed = block.committed.load();
        let block_state = block.allocate_entry();
        match block_state {
            BlockState::Allocated(allocated_offset) => {
                block.commit_entry(allocated_offset, vec![0, 1]);
                assert_eq!(committed + 1, block.committed.load());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn should_reserve_entry_return_done_when_reserved_is_greater_than_block_size() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        let version = 2;
        block
            .reserved
            .load_and_add(to_variable_value_from_sub_components(
                version,
                block_size as u64 + 1,
                offset_bits_size,
            ));
        let block_state = block.reserve_entry();
        assert_eq!(BlockState::Done(version), block_state);
    }

    #[test]
    fn should_reserve_entry_return_done_when_reserved_equals_to_block_size() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        let version = 1;
        block
            .reserved
            .load_and_add(to_variable_value_from_sub_components(
                version,
                block_size as u64,
                offset_bits_size,
            ));
        let block_state = block.reserve_entry();
        assert_eq!(BlockState::Done(version), block_state);
    }

    #[test]
    fn should_reserve_entry_return_no_entry_when_reserved_equals_committed() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        let version = 1;
        let offset_version_value =
            to_variable_value_from_sub_components(version, block_size as u64 - 1, offset_bits_size);
        block.reserved.load_and_add(offset_version_value);
        block.committed.load_and_add(offset_version_value);
        let block_state = block.reserve_entry();
        assert_eq!(BlockState::NoEntry(), block_state);
    }

    #[test]
    fn should_reserve_entry_return_not_available_when_committed_does_not_equal_to_allocated() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        let version = 1;
        let offset_version_value =
            to_variable_value_from_sub_components(version, block_size as u64 - 1, offset_bits_size);
        block.reserved.load_and_add(offset_version_value);
        block.committed.load_and_add(offset_version_value - 1);
        block.allocated.load_and_add(offset_version_value - 2);
        let block_state = block.reserve_entry();
        assert_eq!(BlockState::NotAvailable(), block_state);
    }

    #[test]
    fn should_reserve_entry_return_reserved_when_committed_equals_to_block_size() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        let version = 1;
        let variable_offset = block_size as u64 - 1;
        let offset_version_value =
            to_variable_value_from_sub_components(version, variable_offset, offset_bits_size);
        block.reserved.load_and_add(offset_version_value);
        block.committed.load_and_add(offset_version_value + 1);
        block.allocated.load_and_add(offset_version_value - 2);
        let block_state = block.reserve_entry();
        assert_eq!(BlockState::Reserved(variable_offset), block_state);
    }

    #[test]
    fn should_consume_a_reserved_entry() {
        let initial_offset = 0;
        let block_size = 16;
        let offset_bits_size = 16;
        let block: Block<Vec<u8>> = Block::init(block_size, initial_offset, offset_bits_size);
        let block_state = block.allocate_entry();
        match block_state {
            BlockState::Allocated(allocated_offset) => {
                let data = vec![0, 1];
                let _ = block.commit_entry(allocated_offset, data.clone());
                let reserved_state = block.reserve_entry();
                match reserved_state {
                    BlockState::Reserved(reserved_offset) => {
                        let consumed_value = block.consumed.load();
                        let data_consumed = block.consume_entry(reserved_offset);
                        assert_eq!(data, data_consumed);
                        assert_eq!(consumed_value + 1, block.consumed.load());
                    }
                    state => {
                        println!("{:?}", state);
                        unreachable!()
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn should_enqueue_an_entry_when_space_available() {
        let capacity = 10;
        let queue = Queue::with_capacity(capacity);
        let data: Vec<u8> = vec![0, 1];
        let result = queue.enqueue(data);
        assert_eq!(EnqueueState::Ok(), result);
    }

    #[test]
    fn should_enqueue_an_entry_when_first_block_is_full() {
        let capacity = 10;
        let queue = Queue::with_capacity(capacity);
        let data: Vec<u8> = vec![0, 1];
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let result = queue.enqueue(data);
        assert_eq!(EnqueueState::Ok(), result);
    }

    #[test]
    fn should_not_enqueue_an_entry_when_all_blocks_are_full() {
        let capacity = 10;
        let queue = Queue::with_capacity(capacity);
        let data: Vec<u8> = vec![0, 1];
        for _ in 0..12 {
            let _ = queue.enqueue(data.clone());
        }
        let returned_data = data.clone();
        let result = queue.enqueue(data);
        assert_eq!(EnqueueState::Full(returned_data), result);
    }
    #[test]
    fn should_dequeue_an_entry_when_data_available() {
        let capacity = 10;
        let queue = Queue::with_capacity(capacity);
        let data: Vec<u8> = vec![0, 1];
        let expected_data = data.clone();
        let _ = queue.enqueue(data);
        let dequeue_state = queue.dequeue();
        assert_eq!(DequeueState::Ok(expected_data), dequeue_state);
    }

    #[test]
    fn should_dequeue_an_entry_when_first_block_is_consumed() {
        let capacity = 10;
        let queue = Queue::with_capacity(capacity);
        let data: Vec<u8> = vec![0, 1];
        let expected_data = data.clone();
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data);
        let _ = queue.dequeue();
        let _ = queue.dequeue();
        let _ = queue.dequeue();
        let _ = queue.dequeue();
        let dequeue_state = queue.dequeue();
        assert_eq!(DequeueState::Ok(expected_data), dequeue_state);
    }

    #[test]
    fn should_not_dequeue_an_entry_when_all_data_is_consumed() {
        let capacity = 10;
        let queue = Queue::with_capacity(capacity);
        let data: Vec<u8> = vec![0, 1];
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.enqueue(data.clone());
        let _ = queue.dequeue();
        let _ = queue.dequeue();
        let _ = queue.dequeue();
        let _ = queue.dequeue();
        let dequeue_state = queue.dequeue();
        assert_eq!(DequeueState::Empty(), dequeue_state);
    }
}
