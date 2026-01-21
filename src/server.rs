use crate::handler::Handler;
use crate::io::{BufferPool};
use crate::net::{Connection, ConnectionState};
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token, Waker};
use slab::Slab;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const LISTENER_TOKEN: Token = Token(0);
const WAKER_TOKEN: Token = Token(1);
const SLAB_OFFSET: usize = 2;

pub struct Server<H: Handler> {
    poll: Poll,
    events: Events,
    listener: TcpListener,
    connections: Slab<Connection>,
    buffer_pool: BufferPool,
    handler: H,
}

impl<H: Handler> Server<H> {
    pub fn new(addr: &str, handler: H) -> std::io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);
        let mut listener = TcpListener::bind(addr.parse().unwrap())?;

        // Register the listener to know when new clients connect
        poll.registry()
            .register(&mut listener, Token(0), Interest::READABLE)?;

        Ok(Self {
            poll,
            events,
            listener,
            connections: Slab::with_capacity(1024),
            buffer_pool: BufferPool::new(1024, 8192), // 1024 buffers of 8KB
            handler,
        })
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        // Stop flag and Waker to gracefully kill application
        let should_stop = Arc::new(AtomicBool::new(false));
        let r = should_stop.clone();
        let waker = Arc::new(Waker::new(self.poll.registry(), WAKER_TOKEN)?);
        let waker_for_signal = waker.clone();

        // set signal handler to Ctrl-C
        ctrlc::set_handler(move || {
            println!("\nShutdown signal received...");
            r.store(true, Ordering::SeqCst);
            waker_for_signal.wake().expect("Failed to wake event loop");
        })
        .expect("Error setting Ctrl-C handler");

        loop {
            let mut to_remove: Vec<Token> = Vec::new();
            self.poll.poll(&mut self.events, None)?;
            for event in self.events.iter() {
                println!("{:?}", event);

                match event.token() {
                    LISTENER_TOKEN => {
                        // Accept new connection
                        match self.listener.accept() {
                            Ok((mut stream, _addr)) => {
                                let entry = self.connections.vacant_entry();
                                let token = Token(entry.key() + SLAB_OFFSET);
                                self.poll.registry().register(
                                    &mut stream,
                                    token,
                                    Interest::READABLE.add(Interest::WRITABLE),
                                )?;
                                entry.insert(Connection::new(
                                    stream,
                                    self.buffer_pool.checkout(),
                                    self.buffer_pool.checkout(),
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
                        if let Some(conn) = self.connections.get_mut(usize::from(token) - SLAB_OFFSET) {
                            conn.process(event, &self.handler);
                            if ConnectionState::Closed == *conn.state() {
                                to_remove.push(token);
                            }
                        }
                    }
                }
            }

            for token in to_remove {
                // TODO use try_remove
                let mut conn = self.connections.remove(usize::from(token) - SLAB_OFFSET);
                self.poll.registry().deregister(conn.socket())?;

                // Return buffer to the buffer pool
                let (buf1, buf2) = conn.get_buffers();
                self.buffer_pool.return_buffer(buf1);
                self.buffer_pool.return_buffer(buf2);

                println!("Connection closed and removed.");
            }
        }
    }
}
