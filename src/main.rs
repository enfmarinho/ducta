use ducta::{
    http::{Request, Response},
    Server,
};

fn main() -> std::io::Result<()> {
    let _ = Server::new("127.0.0.1:8080", |req: Request| {
        // Log the request path
        println!("Request: {} {}", req.method, req.path);

        Response::new(200).with_body("Hello from Duca!")
    })?
    .run();

    Ok(())
}
