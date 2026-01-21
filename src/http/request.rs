pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub version: u8,
    pub headers: &'a [httparse::Header<'a>],
}

impl<'a> Request<'a> {
    pub fn get_header(&self, name: &str) -> Option<&'a [u8]> {
        self.headers.iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value)
    }
}
