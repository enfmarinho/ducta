use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};

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
pub fn run() {
    let listener = TcpListener::bind("127.0.0.1:8080").expect("failed to bind to address");

    listener
        .set_nonblocking(true)
        .expect("failed to set non-blocking");

    println!("Listening on 127.0.0.1:8080");

    let mut connections: Vec<Connection> = Vec::new();

    loop {
        // Accept new connections
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("New connection from {}", addr);
                stream
                    .set_nonblocking(true)
                    .expect("failed to set non-blocking");

                connections.push(Connection::new(stream));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No new connections
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }

        // Handle existing connections
        for conn in connections.iter_mut() {
            match conn.state {
                ConnectionState::Reading => conn.read(),
                ConnectionState::Writing => conn.write(),
                ConnectionState::Closed => {}
            }
        }

        // Remove closed connections
        connections.retain(|c| !matches!(c.state, ConnectionState::Closed));
    }
}
