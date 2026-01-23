# Ducta

Ducta is an experimental, non-blocking HTTP server written in Rust. It is a **learning-oriented systems project** exploring low-level Linux I/O primitives, zero-copy request parsing, and explicit per-connection state management.

The goal is to develop a **high-performance, deterministic server** while deepening understanding of Linux asynchronous I/O and Rust memory-safety patterns.

---

## Implemented Features

- Minimal non-blocking HTTP server using **Mio** and **Slab** for connection management  
- Per-connection **state machine** for reads/writes  
- HTTP abstractions with a **Handler trait** to generate responses from requests  
- **BufferPool** for efficient memory reuse and reduced heap allocations  
- Reuse of open TCP connections and **TCP_NODELAY** for latency optimization  
- Graceful shutdown handling on **SIGINT**  
- Example usage demonstrating a simple HTTP GET request  
- Modular code structure for easy extension and experimentation  

---

## Current Focus

- Improving server **throughput and latency** via careful state machine design  
- Experimenting with **epoll** and **io_uring** backends for high-performance asynchronous I/O  
- Supporting **zero-copy HTTP request parsing** to minimize memory overhead  
- Maintaining **deterministic and concurrent connection handling**  

---

## Planned Features

- Full HTTP parsing and standard-compliant responses  
- High-level routing and middleware support  
- TLS / HTTPS support  
- Production-level benchmarks and throughput guarantees  
- Advanced connection pooling and backpressure management  

---

## Status

**Experimental / Early Development** — API is unstable and not production-ready.  

---

## Learning Objectives

- Deepen understanding of **Linux asynchronous I/O** and event-driven architecture  
- Practice **Rust systems programming** with explicit memory and concurrency management, such as buffer and thread pools  
- Explore **high-throughput networking patterns** for future real-time and embedded applications

