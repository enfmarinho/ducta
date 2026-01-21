use std::net::SocketAddr;

fn main() {
    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let _ = ducta::run(addr);
}
