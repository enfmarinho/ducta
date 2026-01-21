use crate::handler::Handler;
use crate::io::{BufferPool, BUFFER_STANDARD_SIZE};
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
            .register(&mut listener, LISTENER_TOKEN, Interest::READABLE)?;

        Ok(Self {
            poll,
            events,
            listener,
            connections: Slab::with_capacity(1024),
            buffer_pool: BufferPool::new(1024, BUFFER_STANDARD_SIZE),
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
            eprintln!("\nShutdown signal received...");
            r.store(true, Ordering::SeqCst);
            waker_for_signal.wake().expect("Failed to wake event loop");
        })
        .expect("Error setting Ctrl-C handler");

        let mut to_remove: Vec<Token> = Vec::new();

        loop {
            to_remove.clear();

            self.poll.poll(&mut self.events, None)?;

            for event in self.events.iter() {
                match event.token() {
                    LISTENER_TOKEN => {
                        // Accept as many connections as possible
                        loop {
                            match self.listener.accept() {
                                Ok((mut stream, _addr)) => {
                                    let entry = self.connections.vacant_entry();
                                    let token = Token(entry.key() + SLAB_OFFSET);

                                    if let Err(e) = self.poll.registry().register(
                                        &mut stream,
                                        token,
                                        Interest::READABLE,
                                    ) {
                                        eprintln!("Failed to register connection: {}", e);
                                        continue;
                                    }

                                    entry.insert(Connection::new(
                                        stream,
                                        self.buffer_pool.checkout(),
                                        self.buffer_pool.checkout(),
                                    ));
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    // No more connections to accept right now
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Accept error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    WAKER_TOKEN => {
                        if should_stop.load(Ordering::SeqCst) {
                            eprintln!("Graceful shutdown starting...");
                            return Ok(());
                        }
                    }
                    token => {
                        let conn_idx = usize::from(token) - SLAB_OFFSET;
                        if let Some(conn) = self.connections.get_mut(conn_idx) {
                            if let Some(new_interest) = conn.process(event, &self.handler) {
                                self.poll.registry().reregister(
                                    conn.socket(),
                                    token,
                                    new_interest,
                                )?;
                            }

                            if *conn.state() == ConnectionState::Closed {
                                to_remove.push(token);
                            }
                        }
                    }
                }
            }

            // Clean up closed connections
            for token in &to_remove {
                let conn_idx = usize::from(*token) - SLAB_OFFSET;
                let mut conn = self.connections.remove(conn_idx);

                if let Err(e) = self.poll.registry().deregister(conn.socket()) {
                    eprintln!("Failed to deregister connection: {}", e);
                }

                // Return buffer to the buffer pool
                let (buf1, buf2) = conn.get_buffers();
                self.buffer_pool.return_buffer(buf1);
                self.buffer_pool.return_buffer(buf2);

                // TODO this can be removed, just for visualization
                println!("Removing connection at index: {}", conn_idx);
            }
        }
    }
}
