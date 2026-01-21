mod io;

use bytes::BytesMut;
use io::buffer_pool::BufferPool;

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};
use slab::Slab;
use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::io::buffer_pool::BUFFER_STANDARD_SIZE;

const LISTENER_TOKEN: Token = Token(0);
const WAKER_TOKEN: Token = Token(1);
const SLAB_OFFSET: usize = 2;

enum ConnectionState {
    Reading,
    Writing,
    Closed,
}

struct Connection {
    read_buffer: BytesMut,
    write_buffer: BytesMut,
    state: ConnectionState,
    socket: TcpStream,
}

impl Connection {
    fn new(socket: TcpStream, read_buffer: BytesMut, write_buffer: BytesMut) -> Self {
        Connection {
            state: ConnectionState::Reading,
            read_buffer,
            write_buffer,
            socket,
        }
    }

    fn read(&mut self) {
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

    fn write(&mut self) {
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
}

pub fn run(addr: SocketAddr) -> std::io::Result<()> {
    let mut listener = TcpListener::bind(addr)?;
    println!("Listening on {}", addr);

    let mut poll = Poll::new()?;
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)?;

    // Stop flag and Waker to gracefully kill application
    let should_stop = Arc::new(AtomicBool::new(false));
    let r = should_stop.clone();
    let waker = Arc::new(Waker::new(poll.registry(), WAKER_TOKEN)?);
    let waker_for_signal = waker.clone();

    // set signal handler to Ctrl-C
    ctrlc::set_handler(move || {
        println!("\nShutdown signal received...");
        r.store(true, Ordering::SeqCst);
        waker_for_signal.wake().expect("Failed to wake event loop");
    })
    .expect("Error setting Ctrl-C handler");

    let mut events = Events::with_capacity(1024);

    let mut connections: Slab<Connection> = Slab::with_capacity(1024);

    let mut buffer_pool = BufferPool::new(1024, BUFFER_STANDARD_SIZE);

    loop {
        let mut to_remove: Vec<Token> = Vec::new();
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            println!("{:?}", event);

            match event.token() {
                LISTENER_TOKEN => {
                    // Accept new connection
                    match listener.accept() {
                        Ok((mut stream, _addr)) => {
                            let entry = connections.vacant_entry();
                            let token = Token(entry.key() + SLAB_OFFSET);
                            poll.registry().register(
                                &mut stream,
                                token,
                                Interest::READABLE.add(Interest::WRITABLE),
                            )?;
                            entry.insert(Connection::new(
                                stream,
                                buffer_pool.checkout(),
                                buffer_pool.checkout(),
                            ));
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => eprintln!("Accept error: {}", e),
                    }
                }
                WAKER_TOKEN => {
                    println!("Waker triggered! Checking exit flag.");
                    if should_stop.load(Ordering::SeqCst) {
                        println!("Graceful shutdown starting...");
                        return Ok(()); // Exit the run function
                    }
                }
                token => {
                    if let Some(conn) = connections.get_mut(usize::from(token) - SLAB_OFFSET) {
                        if event.is_readable() {
                            conn.read();
                        }
                        if event.is_writable() {
                            conn.write();
                        }

                        if let ConnectionState::Closed = conn.state {
                            to_remove.push(token);
                        }
                    }
                }
            }
        }

        for token in to_remove {
            // TODO use try_remove
            let mut conn = connections.remove(usize::from(token) - SLAB_OFFSET);
            poll.registry().deregister(&mut conn.socket)?;

            // Return buffer to the buffer pool
            let read_buf = conn.read_buffer;
            let write_buf = conn.write_buffer;
            buffer_pool.return_buffer(read_buf);
            buffer_pool.return_buffer(write_buf);

            println!("Connection closed and removed.");
        }
    }
}
