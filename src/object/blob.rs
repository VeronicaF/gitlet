use std::fmt::Display;

pub struct Blob<'a> {
    pub data: &'a [u8],
}

impl Blob<'_> {
    pub fn new(data: &[u8]) -> Blob {
        Blob { data }
    }
}

impl Display for Blob<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = String::from_utf8_lossy(self.data);
        write!(f, "{}", data)
    }
}
