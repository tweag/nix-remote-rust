use anyhow::anyhow;
use nix_remote::{
    DaemonHandle,
    NixRead,
    NixReadExt,
    NixString,
    NixWrite,
    NixWriteExt,
    PROTOCOL_VERSION,
    Result,
    WORKER_MAGIC_1,
    WORKER_MAGIC_2,
    stderr,
    serialize,
    worker_op::WorkerOp,
    worker_op::Stream,
};
use std::io::Read;
use std::io::Write;
/// A proxy to the nix daemon.
///
/// This doesn't currently *do* very much, it just inspects the protocol as it goes past.
/// But it can be used to test our protocol implementation.
pub struct NixProxy<R, W> {
    pub read: NixRead<R>,
    pub write: NixWrite<W>,
    proxy: DaemonHandle,
}

impl<R: Read, W: Write> NixProxy<R, W> {
    pub fn new(r: R, w: W) -> Self {
        Self {
            read: NixRead { inner: r },
            write: NixWrite { inner: w },
            proxy: DaemonHandle::new(),
        }
    }
}

impl<R: Read, W: Write> NixProxy<R, W> {
    // Wait for an initialization message from the client, and perform
    // the version negotiation.
    //
    // Returns the client version.
    pub fn handshake(&mut self) -> Result<u64> {
        let magic = self.read.read_u64()?;
        if magic != WORKER_MAGIC_1 {
            eprintln!("{magic:x}");
            eprintln!("{WORKER_MAGIC_1:x}");
            todo!("handle error: protocol mismatch 1");
        }

        self.write.write_u64(WORKER_MAGIC_2)?;
        self.write.write_u64(PROTOCOL_VERSION.into())?;
        self.write.flush()?;

        let client_version = self.read.read_u64()?;

        if client_version < PROTOCOL_VERSION.into() {
            Err(anyhow!("Client version {client_version} is too old"))?;
        }

        // TODO keep track of number of WorkerOps performed
        let mut _op_count: u64 = 0;

        let _obsolete_cpu_affinity = self.read.read_u64()?;
        let _obsolete_reserve_space = self.read.read_u64()?;
        self.write.write_string("rust-nix-bazel-0.1.0".as_bytes())?;
        self.write.flush()?;
        Ok(PROTOCOL_VERSION.into())
    }

    fn forward_stderr(&mut self) -> Result<()> {
        loop {
            let msg: stderr::Msg = self.proxy.child_out.read_nix()?;
            self.write.inner.write_nix(&msg)?;
            eprintln!("read stderr msg {msg:?}");
            self.write.inner.flush()?;

            if msg == stderr::Msg::Last(()) {
                break;
            }
        }
        Ok(())
    }

    /// Process a remote nix connection.
    pub fn process_connection(&mut self) -> Result<()>
    where
        W: Send,
    {
        let client_version = self.handshake()?;

        // Shake hands with the daemon that we're proxying.
        self.proxy.child_in.write_nix(&WORKER_MAGIC_1)?;
        self.proxy.child_in.flush()?;
        let magic: u64 = self.proxy.child_out.read_nix()?;
        if magic != WORKER_MAGIC_2 {
            Err(anyhow!("unexpected WORKER_MAGIC_2: got {magic:x}"))?;
        }
        let protocol_version: u64 = self.proxy.child_out.read_nix()?;
        if protocol_version < PROTOCOL_VERSION.into() {
            Err(anyhow!(
                "unexpected protocol version: got {protocol_version}"
            ))?;
        }
        self.proxy.child_in.write_nix(&client_version)?;
        self.proxy.child_in.write_nix(&0u64)?; // cpu affinity, obsolete
        self.proxy.child_in.write_nix(&0u64)?; // reserve space, obsolete
        self.proxy.child_in.flush()?;
        let proxy_daemon_version: NixString = self.proxy.child_out.read_nix()?;
        eprintln!(
            "Proxy daemon is: {}",
            String::from_utf8_lossy(proxy_daemon_version.0.as_ref())
        );
        self.forward_stderr()?;

        loop {
            let op = match self.read.inner.read_nix::<WorkerOp>() {
                Err(serialize::Error::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    eprintln!("EOF, closing");
                    break;
                }
                x => x,
            }?;

            eprintln!("read op {op:?}");
            self.proxy.child_in.write_nix(&op).unwrap();
            op.stream(&mut self.read.inner, &mut self.proxy.child_in)
                .unwrap();
            self.proxy.child_in.flush().unwrap();

            self.forward_stderr()?;

            // Read back the actual response.
            op.proxy_response(&mut self.proxy.child_out, &mut self.write.inner)?;
            self.write.inner.flush()?;
        }
        Ok(())
    }
}
