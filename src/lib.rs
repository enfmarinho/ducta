mod handler;
mod http;
mod io;
mod net;

use io::{BufferPool, BUFFER_STANDARD_SIZE};
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token, Waker};
use net::{Connection, ConnectionState};
use slab::Slab;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const LISTENER_TOKEN: Token = Token(0);
const WAKER_TOKEN: Token = Token(1);
const SLAB_OFFSET: usize = 2;

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
                        conn.process(event);
                        if ConnectionState::Closed == *conn.state() {
                            to_remove.push(token);
                        }
                    }
                }
            }
        }

        for token in to_remove {
            // TODO use try_remove
            let mut conn = connections.remove(usize::from(token) - SLAB_OFFSET);
            poll.registry().deregister(conn.socket())?;

            // Return buffer to the buffer pool
            let (buf1, buf2) = conn.get_buffers();
            buffer_pool.return_buffer(buf1);
            buffer_pool.return_buffer(buf2);

            println!("Connection closed and removed.");
        }
    }
}
