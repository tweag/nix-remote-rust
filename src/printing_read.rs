use std::io::Read;

pub struct PrintingRead<R> {
    pub buf: Vec<u8>,
    pub inner: R,
}

impl<R: Read> Read for PrintingRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        eprintln!("reading up to {} bytes...", buf.len());
        let size = self.inner.read(buf)?;
        self.buf.extend_from_slice(&buf[..size]);
        eprintln!("read {:?}", &buf[..size]);
        Ok(size)
    }
}
