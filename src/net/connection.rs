use bytes::BufMut;
use bytes::BytesMut;
use mio::event::Event;

use crate::handler::Handler;
use crate::http::{self, ParseStatus};

use mio::net::TcpStream;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;

const MAX_REQUEST_SIZE: usize = 8192;

#[derive(PartialEq)]
pub enum ConnectionState {
    Reading,
    Writing,
    Closed,
}

pub struct Connection {
    pub read_buffer: BytesMut,
    pub write_buffer: BytesMut,
    state: ConnectionState,
    socket: TcpStream,
}

impl Connection {
    pub fn new(socket: TcpStream, read_buffer: BytesMut, write_buffer: BytesMut) -> Self {
        Connection {
            state: ConnectionState::Reading,
            read_buffer,
            write_buffer,
            socket,
        }
    }

    /// The state machine coordinator.
    /// Dispatches I/O tasks based on current state and event.
    pub fn process<H: Handler>(&mut self, event: &Event, handler: &H) {
        // Handle the read phase
        // We only read if the socket is ready AND we are expecting a request.
        if event.is_readable() && self.state == ConnectionState::Reading {
            match self.read() {
                Ok(0) => {
                    // Clean close: peer shut down the connection
                    self.state = ConnectionState::Closed;
                    return;
                }
                Ok(n) if n > 0 => {
                    // Data arrived, Try to parse it into an HTTP Request.
                    self.handle_request(handler);
                }
                Ok(_) => (), // WouldBlock received, so no more data to read for now
                Err(_) => {
                    // Fatal socket error
                    self.state = ConnectionState::Closed;
                    return;
                }
            }
        }

        // Handles the write phase
        if self.state == ConnectionState::Writing {
            match self.write() {
                Ok(_) => {
                    // write() will set state to Closed once the buffer is fully drained.
                    // If it was a partial write, state remains Writing.
                }
                Err(_) => {
                    self.state = ConnectionState::Closed;
                }
            }
        }
    }

    /// Reads data from the socket directly into the pooled BytesMut.
    /// Returns the number of bytes read in this call.
    pub fn read(&mut self) -> std::io::Result<usize> {
        let mut bytes_read_this_turn = 0;

        loop {
            // Hard limit check
            // Prevents malicious clients from causing Out-Of-Memory (OOM) via Slowloris.
            if self.read_buffer.len() >= MAX_REQUEST_SIZE {
                self.state = ConnectionState::Closed;
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    "Request header too large",
                ));
            }

            // Ensure there is space to read.
            if self.read_buffer.remaining_mut() < 1024 {
                let space_left = MAX_REQUEST_SIZE - self.read_buffer.len();
                // If we can't even fit 1 more byte, stop reading.
                if space_left == 0 {
                    break;
                }

                // Reserve a chunk, but never more than our Hard Limit.
                self.read_buffer.reserve(std::cmp::min(1024, space_left));
            }

            let n = unsafe {
                // Get raw pointer to the start of uninitialized capacity
                let ptr = self.read_buffer.as_mut_ptr().add(self.read_buffer.len());
                let cap = self.read_buffer.capacity() - self.read_buffer.len();

                // Create a slice that points directly to our Pool Memory
                let slice = std::slice::from_raw_parts_mut(ptr, cap);

                match self.socket.read(slice) {
                    Ok(0) => {
                        // Clean EOF: Client closed the connection
                        self.state = ConnectionState::Closed;
                        return Ok(bytes_read_this_turn);
                    }
                    Ok(n) => n,
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                        // No more data in the Kernel's buffer for this event
                        return Ok(bytes_read_this_turn);
                    }
                    Err(e) => {
                        self.state = ConnectionState::Closed;
                        return Err(e);
                    }
                }
            };

            // Commit the bytes, by updating the new end of the read_buffer
            unsafe {
                let new_len = self.read_buffer.len() + n;
                self.read_buffer.set_len(new_len);
            }

            bytes_read_this_turn += n;

            // Loop continues to "drain" the socket until it would block.
        }

        Ok(bytes_read_this_turn)
    }

    /// Writes the contents of write_buffer to the socket.
    /// Returns the number of bytes written in this call.
    pub fn write(&mut self) -> std::io::Result<usize> {
        let mut bytes_written_this_turn = 0;

        // Drain the buffer
        while !self.write_buffer.is_empty() {
            match self.socket.write(&self.write_buffer) {
                Ok(0) => {
                    // A write of 0 usually means the connection was dropped by the peer
                    self.state = ConnectionState::Closed;
                    return Ok(bytes_written_this_turn);
                }
                Ok(n) => {
                    // Zero-copy
                    let _ = self.write_buffer.split_to(n);
                    bytes_written_this_turn += n;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // Backpressure handling: the kernel send buffer is full, so stop writing and
                    // return, staying in the Writing state so the next event triggers this method
                    // again
                    return Ok(bytes_written_this_turn);
                }
                Err(e) => {
                    // Fatal socket error
                    self.state = ConnectionState::Closed;
                    return Err(e);
                }
            }
        }

        // If the loop finishes, the buffer is empty.
        // TODO: when HTTP Keep-Alive is implemented, switch back to Reading instead.
        self.state = ConnectionState::Closed;

        Ok(bytes_written_this_turn)
    }

    /// Attempts to parse and process an HTTP request from the internal read buffer.
    ///
    /// This method implements a zero-copy approach:
    /// 1. It uses `httparse` to find the boundaries of the request without copying data.
    /// 2. If a full request is found, it "consumes" those bytes from the `read_buffer` 
    ///    using `split_to`, which is an O(1) operation that simply moves a pointer.
    /// 3. It then generates a response and transitions the connection state to `Writing`.
    ///
    /// # Pipelining Support
    /// If the `read_buffer` contains more than one request (Pipelining), this method 
    /// only consumes the first request. The remaining bytes stay in the buffer to 
    /// be processed in the next cycle.
    pub fn handle_request<H: Handler>(&mut self, handler:&H) {
        // Storage for headers (httparse needs a place to put references)
        let mut header_storage = [httparse::EMPTY_HEADER; 64];

        // Attempt to parse the read_buffer
        match http::parse_request(&self.read_buffer, &mut header_storage) {
            ParseStatus::Complete(req, amt) => {
                // full HTTP Request

                // For now, ignore the request and just generate a hardcoded response
                let resp = handler.handle(req);

                // Encode the response into the write_buffer
                resp.encode(&mut self.write_buffer);

                // Consume the parsed bytes from the read_buffer
                // If there's extra data (like a second request), it stays in read_buffer
                let _ = self.read_buffer.split_to(amt);

                // Transition state to start sending the data
                self.state = ConnectionState::Writing;
            }
            ParseStatus::Partial => {
                // The HTTP request wasn't fully read, so stay in the Reading state wating for the
                // next event
            }
            ParseStatus::Error(_) => {
                // Malformed HTTP, close the connection
                self.state = ConnectionState::Closed;
            }
        }
    }

    pub fn socket(&mut self) -> &mut TcpStream {
        &mut self.socket
    }

    pub fn state(&self) -> &ConnectionState {
        &self.state
    }

    pub fn get_buffers(self) -> (BytesMut, BytesMut) {
        (self.read_buffer, self.write_buffer)
    }
}
