use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{self, Read, Write};

use worker_op::ValidPathInfo;

pub mod framed_data;
pub mod nar;
pub mod printing_read;
mod serialize;
pub mod stderr;
pub mod worker_op;
use serialize::{NixDeserializer, NixSerializer};

pub use framed_data::FramedData;

use crate::{
    printing_read::PrintingRead,
    serialize::{NixReadExt, NixWriteExt},
    worker_op::{Stream, WorkerOp},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("(De)serialization error: {0}")]
    Deser(#[from] serialize::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
#[serde(transparent)]
pub struct StorePath(pub NixString);

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
#[serde(transparent)]
pub struct Path(pub NixString);

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
#[serde(transparent)]
pub struct DerivedPath(pub NixString);

/// Strings in the nix protocol are not necessarily UTF-8.
///
/// This type marks a byte buffer that's expected to be "stringy".
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
#[serde(transparent)]
pub struct NixString(pub ByteBuf);

impl std::fmt::Debug for NixString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

const WORKER_MAGIC_1: u64 = 0x6e697863;
const WORKER_MAGIC_2: u64 = 0x6478696f;
const PROTOCOL_VERSION: DaemonVersion = DaemonVersion {
    major: 1,
    minor: 34,
};

pub struct NixProxy {
    child_in: std::process::ChildStdin,
    child_out: std::process::ChildStdout,
}

impl NixProxy {
    pub fn new() -> Self {
        let mut child = std::process::Command::new("nix-daemon")
            .arg("--stdio")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        Self {
            child_in: child.stdin.take().unwrap(),
            child_out: child.stdout.take().unwrap(),
        }
    }

    pub fn write_u64(&mut self, n: u64) -> Result<()> {
        self.child_in.write_all(&n.to_le_bytes())?;
        Ok(())
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.child_out.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn flush(&mut self) -> Result<()> {
        Ok(self.child_in.flush()?)
    }

    pub fn read_string(&mut self) -> Result<Vec<u8>> {
        let mut deserializer = NixDeserializer {
            read: &mut self.child_out,
        };
        let bytes = ByteBuf::deserialize(&mut deserializer)?;
        Ok(bytes.into_vec())
    }
}

pub struct NixReadWrite<R, W> {
    pub read: NixStoreRead<R>,
    pub write: NixStoreWrite<W>,
    pub proxy: NixProxy,
}

pub struct NixStoreRead<R> {
    pub inner: R,
}

pub struct NixStoreWrite<W> {
    pub inner: W,
}

/// The serialization format of PathSet and StorePathSet is the same, but there's a semantic
/// difference: these paths are not in the store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    pub paths: Vec<Path>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorePathSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    pub paths: Vec<StorePath>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StringSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    pub paths: Vec<NixString>,
}

pub type Realisation = NixString;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RealisationSet {
    pub realisations: Vec<Realisation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NarHash {
    pub data: ByteBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidPathInfoWithPath {
    pub path: StorePath,
    pub info: ValidPathInfo,
}

impl<R: Read> NixStoreRead<R> {
    pub fn read_u64(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.inner.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
}

impl<W: Write> NixStoreWrite<W> {
    fn write_u64(&mut self, n: u64) -> Result<()> {
        self.inner.write_all(&n.to_le_bytes())?;
        Ok(())
    }

    fn write_string(&mut self, s: &[u8]) -> Result<()> {
        self.write_u64(s.len() as _)?;
        self.inner.write_all(s)?;

        if s.len() % 8 > 0 {
            let padding = 8 - s.len() % 8;
            let pad_buf = [0; 8];
            self.inner.write_all(&pad_buf[..padding])?;
        }

        Ok(())
    }

    fn write(&mut self, data: &impl Serialize) -> Result<()> {
        data.serialize(&mut NixSerializer {
            write: &mut self.inner,
        })?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(self.inner.flush()?)
    }
}

impl<R: Read, W: Write> NixReadWrite<R, W> {
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

    /// Process a remote nix connection.
    /// Reimplement Daemon::processConnection from nix/src/libstore/daemon.cc
    pub fn process_connection(&mut self, proxy_to_nix: bool) -> Result<()>
    where
        W: Send,
    {
        let client_version = self.handshake()?;

        if proxy_to_nix {
            self.proxy.write_u64(WORKER_MAGIC_1)?;
            self.proxy.flush()?;
            let magic = self.proxy.read_u64()?;
            if magic != WORKER_MAGIC_2 {
                Err(anyhow!("unexpected WORKER_MAGIC_2: got {magic:x}"))?;
            }
            let protocol_version = self.proxy.read_u64()?;
            if protocol_version < PROTOCOL_VERSION.into() {
                Err(anyhow!(
                    "unexpected protocol version: got {protocol_version}"
                ))?;
            }
            self.proxy.write_u64(client_version)?;
            self.proxy.write_u64(0)?; // cpu affinity, obsolete
            self.proxy.write_u64(0)?; // reserve space, obsolete
            self.proxy.flush()?;
            let proxy_daemon_version = self.proxy.read_string()?;
            eprintln!(
                "Proxy daemon is: {}",
                String::from_utf8_lossy(proxy_daemon_version.as_ref())
            );
            // FIXME: copy-paste
            loop {
                eprintln!("trying to read stderr::Msg");
                let mut r = PrintingRead {
                    buf: Vec::new(),
                    inner: &mut self.proxy.child_out,
                };
                let msg: stderr::Msg = r.read_nix()?;
                self.write.inner.write_nix(&msg)?;
                eprintln!("read stderr msg {msg:?}");
                self.write.inner.flush()?;

                if msg == stderr::Msg::Last(()) {
                    break;
                }
            }
        } else {
            self.write.write(&stderr::Msg::Last(()))?;
        }

        loop {
            let mut read = NixStoreRead {
                inner: printing_read::PrintingRead {
                    buf: Vec::new(),
                    inner: &mut self.read.inner,
                },
            };

            let op = match read.inner.read_nix::<WorkerOp>() {
                Err(serialize::Error::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    eprintln!("EOF, closing");
                    break;
                }
                x => x,
            }?;

            // Check that the re-serialization of the op we just read is equivalent
            // to the original bytes.
            let mut buf = Vec::new();
            buf.write_nix(&op).unwrap();
            if buf != read.inner.buf {
                eprintln!("mismatch!");
                eprintln!("{buf:?}");
                eprintln!("{:?}", read.inner.buf);
                panic!();
            }

            eprintln!("read op {op:?}");
            self.proxy.child_in.write_nix(&op).unwrap();
            op.stream(&mut read.inner, &mut self.proxy.child_in)
                .unwrap();
            self.proxy.child_in.flush().unwrap();

            // Read back stderr messages from the remote daemon.
            loop {
                let msg: stderr::Msg = self.proxy.child_out.read_nix()?;
                self.write.inner.write_nix(&msg)?;
                eprintln!("read stderr msg {msg:?}");
                self.write.inner.flush()?;

                if msg == stderr::Msg::Last(()) {
                    break;
                }
            }

            // Read back the actual response.
            op.proxy_response(&mut self.proxy.child_out, &mut self.write.inner)?;
            self.write.inner.flush()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DaemonVersion {
    major: u8,
    minor: u8,
}

impl From<u64> for DaemonVersion {
    fn from(x: u64) -> Self {
        let major = ((x >> 8) & 0xff) as u8;
        let minor = (x & 0xff) as u8;
        Self { major, minor }
    }
}

impl From<DaemonVersion> for u64 {
    fn from(DaemonVersion { major, minor }: DaemonVersion) -> Self {
        ((major as u64) << 8) | minor as u64
    }
}
