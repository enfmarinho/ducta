use crate::http::{Request, Response};

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, req: Request) -> Response;
}

// Automatically implement Handler for any function that matches the signature
impl<F> Handler for F 
where 
    F: Fn(Request) -> Response + Send + Sync + 'static 
{
    fn handle(&self, req: Request) -> Response {
        (self)(req)
    }
}
