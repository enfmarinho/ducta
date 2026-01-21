use crate::http::request::Request;

pub enum ParseStatus<'a> {
    Complete(Request<'a>, usize), // The request and the total length of headers
    Partial,                      // Not enough data yet
    Error(httparse::Error),
}

pub fn parse_request<'a>(buffer: &'a [u8], headers_storage: &'a mut [httparse::Header<'a>]) -> ParseStatus<'a> {
    let mut req = httparse::Request::new(headers_storage);

    match req.parse(buffer) {
        Ok(httparse::Status::Complete(amt)) => {
            ParseStatus::Complete(
                Request {
                    method: req.method.unwrap_or(""),
                    path: req.path.unwrap_or(""),
                    version: req.version.unwrap_or(1),
                    headers: req.headers,
                },
                amt,
            )
        }
        Ok(httparse::Status::Partial) => ParseStatus::Partial,
        Err(e) => ParseStatus::Error(e),
    }
}

