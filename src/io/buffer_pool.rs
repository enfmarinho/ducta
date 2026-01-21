use bytes::BytesMut;
use std::collections::VecDeque;

pub const BUFFER_STANDARD_SIZE: usize = 4096; // 4kb
const BUFFER_DANGER_SIZE: usize = 65536; // 64KB

pub struct BufferPool {
    /// Pool of buffers
    pool: VecDeque<BytesMut>,

    /// Size of each buffer
    buffer_size: usize,
}

impl BufferPool {
    /// Initialize the buffer pool with a fixed number of buffers
    pub fn new(initial_capacitiy: usize, buffer_size: usize) -> Self {
        let mut pool = VecDeque::with_capacity(initial_capacitiy);
        for _ in 0..initial_capacitiy {
            pool.push_back(BytesMut::with_capacity(buffer_size));
        }

        BufferPool { pool, buffer_size }
    }

    /// Take a buffer from the pool. If none is available, allocate a new one and return it
    pub fn checkout(&mut self) -> BytesMut {
        self.pool.pop_front().unwrap_or_else(|| {
            // Fallback: if no buffer is available in the pool allocate and return a new one
            BytesMut::with_capacity(self.buffer_size)
        })
    }

    /// Return a buffer to the pool for reuse
    pub fn return_buffer(&mut self, mut buf: BytesMut) {
        buf.clear(); // Resets the length to 0, but keeps the allocated memory

        if buf.capacity() < BUFFER_DANGER_SIZE {
            self.pool.push_back(buf);
        } else {
            self.pool.push_back(BytesMut::with_capacity(self.buffer_size));
        }
    }
}
