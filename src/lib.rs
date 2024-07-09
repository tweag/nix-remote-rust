use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serialize::NixSerializer;
use std::{
    ffi::OsStr,
    io::{Read, Write},
    os::unix::prelude::OsStrExt,
    string::FromUtf8Error,
};

use worker_op::ValidPathInfo;

pub mod framed_data;
pub mod nar;
pub mod serialize;
pub mod stderr;
pub mod worker_op;

pub use serialize::{NixReadExt, NixWriteExt};

use crate::worker_op::{Stream, WorkerOp};

pub fn to_writer<W: std::io::Write, T: ?Sized + Serialize>(
    mut writer: W,
    value: &T,
) -> serialize::Result<()> {
    writer.write_nix(value)
}

pub fn to_vec<T: ?Sized + Serialize>(value: &T) -> serialize::Result<Vec<u8>> {
    let mut ret = Vec::new();
    ret.write_nix(value)?;
    Ok(ret)
}

pub fn from_reader<R: std::io::Read, T: serde::de::DeserializeOwned>(
    mut reader: R,
) -> serialize::Result<T> {
    reader.read_nix()
}

// TODO: getting a proper zero-copy version of this requires sorting out the lifetimes in serializer.
// Not a big priority, since none of the Nix protocol types support borrowed buffers yet
pub fn from_bytes<T: serde::de::DeserializeOwned>(mut bytes: &[u8]) -> serialize::Result<T> {
    bytes.read_nix()
}

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

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug, Eq, Hash)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[serde(transparent)]
pub struct StorePath(pub NixString);

impl AsRef<[u8]> for StorePath {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[serde(transparent)]
pub struct Path(pub NixString);

impl AsRef<[u8]> for Path {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for Path {
    fn as_ref(&self) -> &OsStr {
        OsStr::from_bytes(self.as_ref())
    }
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[serde(transparent)]
pub struct DerivedPath(pub NixString);

impl AsRef<[u8]> for DerivedPath {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// A string from nix.
///
/// Strings in the nix protocol are not necessarily UTF-8, so this is
/// different from the rust standard `String`.
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct NixString(pub ByteBuf);

impl NixString {
    pub fn to_string(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.0.as_slice().to_owned())
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        NixString(ByteBuf::from(bytes.to_vec()))
    }
}

impl From<String> for NixString {
    fn from(s: String) -> NixString {
        NixString(ByteBuf::from(s.into_bytes()))
    }
}

impl From<Vec<u8>> for NixString {
    fn from(s: Vec<u8>) -> NixString {
        NixString(ByteBuf::from(s))
    }
}

impl std::fmt::Debug for NixString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

impl AsRef<[u8]> for NixString {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for NixString {
    fn as_ref(&self) -> &OsStr {
        OsStr::from_bytes(self.as_ref())
    }
}

const WORKER_MAGIC_1: u64 = 0x6e697863;
const WORKER_MAGIC_2: u64 = 0x6478696f;
const PROTOCOL_VERSION: DaemonVersion = DaemonVersion {
    major: 1,
    minor: 34,
};

struct DaemonHandle {
    child_in: std::process::ChildStdin,
    child_out: std::process::ChildStdout,
}

impl DaemonHandle {
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
}

impl Default for DaemonHandle {
    fn default() -> Self {
        Self::new()
    }
}

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

/// A wrapper around a `std::io::Read`, adding support for the nix wire format.
pub struct NixRead<R> {
    pub inner: R,
}

/// A wrapper around a `std::io::Write`, adding support for the nix wire format.
pub struct NixWrite<W> {
    pub inner: W,
}

/// A set of paths.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct PathSet {
    pub paths: Vec<Path>,
}

/// A set of store paths.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct StorePathSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    pub paths: Vec<StorePath>,
}

/// A set of strings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct StringSet {
    pub paths: Vec<NixString>,
}

/// A realisation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct Realisation(pub NixString);

/// A set of realisations.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct RealisationSet {
    pub realisations: Vec<Realisation>,
}

/// A nar hash.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NarHash {
    /// This data has not been validated; this is just copied from the wire.
    pub data: ByteBuf,
}

impl NarHash {
    pub fn from_bytes(bytes: &[u8]) -> NarHash {
        const BASE32_CHARS: &[u8] = b"0123456789abcdfghijklmnpqrsvwxyz";

        let len = (bytes.len() * 8 - 1) / 5 + 1;

        let data = (0..len)
            .rev()
            .map(|n| {
                let b: usize = n * 5;
                let i: usize = b / 8;
                let j: usize = b % 8;
                // bits from the lower byte
                let v1 = bytes[i].checked_shr(j as u32).unwrap_or(0);
                // bits from the upper byte
                let v2 = if i >= bytes.len() - 1 {
                    0
                } else {
                    bytes[i + 1].checked_shl(8 - j as u32).unwrap_or(0)
                };
                let v: usize = (v1 | v2) as usize;
                BASE32_CHARS[v % BASE32_CHARS.len()]
            })
            .collect::<Vec<_>>();

        NarHash {
            data: ByteBuf::from(data),
        }
    }
}

// TODO: This naming is a footgun. CppNix calls the inner one UnkeyedValidPathInfo
// and the outer one ValidPathInfo.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct ValidPathInfoWithPath {
    pub path: StorePath,
    pub info: ValidPathInfo,
}

impl<R: Read> NixRead<R> {
    /// Read an integer from the wire.
    pub fn read_u64(&mut self) -> serialize::Result<u64> {
        self.inner.read_nix()
    }

    /// Read a "string" (really, a byte buffer) from the wire.
    pub fn read_string(&mut self) -> serialize::Result<NixString> {
        self.inner.read_nix()
    }

    /// Read any serializable type from the wire.
    pub fn read_nix(&mut self) -> serialize::Result<()> {
        self.inner.read_nix()
    }
}

impl<W: Write> NixWrite<W> {
    /// Write an integer to the wire.
    pub fn write_u64(&mut self, n: u64) -> serialize::Result<()> {
        self.inner.write_nix(&n)
    }

    /// Write a "string" (really, a byte buffer) to the wire.
    pub fn write_string(&mut self, s: &[u8]) -> serialize::Result<()> {
        NixSerializer {
            write: &mut self.inner,
        }
        .write_byte_buf(s)
    }

    /// Write any serializable type to the wire.
    ///
    /// *Warning*: don't call this with `[u8]` data: that will (attempt to)
    /// serialize a sequence of `u8`s, and then panic because the nix wire
    /// protocol only supports 64-bit integers. If you want to write a byte
    /// buffer, use [`NixWrite::write_string`] instead.
    pub fn write_nix(&mut self, data: &impl Serialize) -> serialize::Result<()> {
        self.inner.write_nix(data)
    }

    /// Flush the underlying writer.
    pub fn flush(&mut self) -> Result<()> {
        Ok(self.inner.flush()?)
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

    pub fn next_op(&mut self) -> Result<Option<WorkerOp>> {
        match self.read.inner.read_nix::<WorkerOp>() {
            Err(serialize::Error::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                Ok(None)
            }
            Err(e) => Err(e.into()),
            Ok(x) => Ok(Some(x)),
        }
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

#[cfg(test)]
impl<'a> arbitrary::Arbitrary<'a> for NixString {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let data: Vec<u8> = Vec::arbitrary(u)?;
        Ok(NixString(ByteBuf::from(data)))
    }
}

#[cfg(test)]
impl<'a> arbitrary::Arbitrary<'a> for NarHash {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let data: Vec<u8> = Vec::arbitrary(u)?;
        Ok(NarHash {
            data: ByteBuf::from(data),
        })
    }
}
