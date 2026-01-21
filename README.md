# Ducta

Ducta is an experimental, non-blocking HTTP server written in Rust.  
It is designed as a learning-oriented systems project focused on explicit
I/O state machines and manual connection management.

**Status: Experimental**
This project is in early development. The API is unstable and the server is
not production-ready.

## Goals

- Explore non-blocking network I/O in Rust
- Implement an explicit per-connection state machine
- Serve as a foundation for future work on epoll / io_uring backends

## Future Goals

- Full HTTP parsing
- High-level routing
- TLS support
- Production performance guarantees

## Usage

```bash
cargo run

