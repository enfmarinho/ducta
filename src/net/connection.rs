use bytes::BytesMut;

use mio::net::TcpStream;
use std::io::Read;
use std::io::Write;

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

    pub fn read(&mut self) {
        if self.read_buffer.capacity() == self.read_buffer.len() {
            self.read_buffer.reserve(1024);
        }

        let n = unsafe {
            let (ptr, len) = (
                self.read_buffer.as_mut_ptr().add(self.read_buffer.len()),
                self.read_buffer.capacity() - self.read_buffer.len(),
            );
            let slice = std::slice::from_raw_parts_mut(ptr, len);

            match self.socket.read(slice) {
                Ok(0) => {
                    self.state = ConnectionState::Closed;
                    return;
                }
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.state = ConnectionState::Closed;
                    return;
                }
            }
        };

        // Increment the length of BytesMut to reflect new data
        unsafe {
            let new_len = self.read_buffer.len() + n;
            self.read_buffer.set_len(new_len);
        }

        // Trigger state change
        self.state = ConnectionState::Writing;
    }

    pub fn write(&mut self) {
        // Hard code answer
        if self.write_buffer.is_empty() {
            let response = b"HTTP/1.1 200 OK\r\n\
                             Content-Length: 5\r\n\
                             Connection: close\r\n\
                             \r\n\
                             Hello";

            self.write_buffer.extend_from_slice(response);
        }

        match self.socket.write(&self.write_buffer) {
            Ok(0) => {
                self.state = ConnectionState::Closed;
            }
            Ok(n) => {
                let _ = self.write_buffer.split_to(n);

                if self.write_buffer.is_empty() {
                    self.state = ConnectionState::Closed;
                }
            }
            Err(e) => {
                eprintln!("Write error: {}", e);
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
