use ducta::{
    http::{Request, Response},
    server::Server,
};

fn main() -> std::io::Result<()> {
    println!("Starting server on 127.0.0.1:8080");

    let _ = Server::new("127.0.0.1:8080", |_req: Request| {
        Response::new(200).with_body(&b"Hello from Duca!"[..])
    })?
    .run();

    Ok(())
}
