use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use slab::Slab;
use std::io::Write;
use std::io::{self, Read};
use std::net::SocketAddr;

const LISTENER_TOKEN: Token = Token(0);
const SLAB_OFFSET: usize = 1;

enum ConnectionState {
    Reading,
    Writing,
    Closed,
}

struct Connection {
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,
    state: ConnectionState,
    socket: TcpStream,
}

impl Connection {
    fn new(socket: TcpStream) -> Self {
        Connection {
            read_buffer: Vec::new(),
            write_buffer: Vec::new(),
            state: ConnectionState::Reading,
            socket,
        }
    }

    fn read(&mut self) {
        let mut buf = [0u8; 1024];

        match self.socket.read(&mut buf) {
            Ok(0) => {
                self.state = ConnectionState::Closed;
            }
            Ok(n) => {
                self.read_buffer.extend_from_slice(&buf[..n]);
                println!("Read {} bytes", n);

                self.state = ConnectionState::Writing;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Do nothing, no data yet
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                self.state = ConnectionState::Closed;
            }
        }
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
                self.write_buffer.drain(..n);

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

pub fn run(addr: SocketAddr) -> io::Result<()> {
    let mut listener = TcpListener::bind(addr)?;
    println!("Listening on {}", addr);

    let mut poll = Poll::new()?;
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)?;

    let mut events = Events::with_capacity(1024);

    let mut connections: Slab<Connection> = Slab::with_capacity(1024);

    loop {
        let mut to_remove: Vec<Token> = Vec::new();
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            println!("{:?}", event);

            if event.token() == LISTENER_TOKEN {
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
                        entry.insert(Connection::new(stream));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => eprintln!("Accept error: {}", e),
                }
            } else if let Some(conn) = connections.get_mut(usize::from(event.token()) - SLAB_OFFSET)
            {
                if event.is_readable() {
                    conn.read();
                }
                if event.is_writable() {
                    conn.write();
                }

                if let ConnectionState::Closed = conn.state {
                    to_remove.push(event.token());
                }
            }
        }

        for token in to_remove {
            let mut conn = connections.remove(usize::from(token) - SLAB_OFFSET);
            poll.registry().deregister(&mut conn.socket)?;
            println!("Connection closed and removed.");
        }
    }
}
