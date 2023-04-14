use std::io::Read;

pub struct PrintingRead<R> {
    pub buf: Vec<u8>,
    pub inner: R,
}

impl<R: Read> Read for PrintingRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let size = self.inner.read(buf)?;
        self.buf.extend_from_slice(&buf[..size]);
        Ok(size)
    }
}
