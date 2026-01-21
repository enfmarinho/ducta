use bytes::{BufMut, BytesMut};

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl Response {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            headers: Vec::new(),
        }
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Converts the struct into raw bytes to be sent over the wire
    pub fn encode(&self, dst: &mut BytesMut) {
        let status_text = match self.status {
            200 => "OK",
            404 => "Not Found",
            _ => "Internal Server Error",
        };

        // Write status line
        dst.put_slice(b"HTTP/1.1 ");
        dst.put_slice(self.status.to_string().as_bytes());
        dst.put_slice(b" ");
        dst.put_slice(status_text.as_bytes());
        dst.put_slice(b"\r\n");

        // Write content-length
        dst.put_slice(b"Content-Length: ");
        dst.put_slice(self.body.len().to_string().as_bytes());
        dst.put_slice(b"\r\n");

        // Default content-type if not provided
        if !self
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("Content-Type"))
        {
            dst.put_slice(b"Content-Type: text/plain\r\n");
        }

        // Write user headers
        for (name, value) in &self.headers {
            dst.put_slice(name.as_bytes());
            dst.put_slice(b": ");
            dst.put_slice(value.as_bytes());
            dst.put_slice(b"\r\n");
        }

        // End headers & body
        dst.put_slice(b"\r\n");
        dst.put_slice(&self.body);
    }
}
