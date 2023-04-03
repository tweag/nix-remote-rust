use anyhow::anyhow;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{self, Read, Write};

pub mod printing_read;
mod serialize;
use serialize::{Deserializer, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("(De)serialization error: {0}")]
    Deser(#[from] serialize::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, FromPrimitive)]
pub enum WorkerOpCode {
    IsValidPath = 1,
    HasSubstitutes = 3,
    QueryPathHash = 4,   // obsolete
    QueryReferences = 5, // obsolete
    QueryReferrers = 6,
    AddToStore = 7,
    AddTextToStore = 8, // obsolete since 1.25, Nix 3.0. Use wopAddToStore
    BuildPaths = 9,
    EnsurePath = 10,
    AddTempRoot = 11,
    AddIndirectRoot = 12,
    SyncWithGC = 13,
    FindRoots = 14,
    ExportPath = 16,   // obsolete
    QueryDeriver = 18, // obsolete
    SetOptions = 19,
    CollectGarbage = 20,
    QuerySubstitutablePathInfo = 21,
    QueryDerivationOutputs = 22, // obsolete
    QueryAllValidPaths = 23,
    QueryFailedPaths = 24,
    ClearFailedPaths = 25,
    QueryPathInfo = 26,
    ImportPaths = 27,                // obsolete
    QueryDerivationOutputNames = 28, // obsolete
    QueryPathFromHashPart = 29,
    QuerySubstitutablePathInfos = 30,
    QueryValidPaths = 31,
    QuerySubstitutablePaths = 32,
    QueryValidDerivers = 33,
    OptimiseStore = 34,
    VerifyStore = 35,
    BuildDerivation = 36,
    AddSignatures = 37,
    NarFromPath = 38,
    AddToStoreNar = 39,
    QueryMissing = 40,
    QueryDerivationOutputMap = 41,
    RegisterDrvOutput = 42,
    QueryRealisation = 43,
    AddMultipleToStore = 44,
    AddBuildLog = 45,
    BuildPathsWithResults = 46,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(transparent)]
pub struct Path(ByteBuf);

impl std::fmt::Debug for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

#[derive(Debug)]
pub struct Resp<T> {
    marker: std::marker::PhantomData<T>,
}

impl<T> Resp<T> {
    fn new() -> Resp<T> {
        Resp {
            marker: std::marker::PhantomData,
        }
    }
}

#[derive(Debug)]
pub enum WorkerOp {
    IsValidPath(Path, Resp<bool>),
    HasSubstitutes(Todo, Resp<Todo>),
    QueryReferrers(Todo, Resp<Todo>),
    AddToStore(AddToStore, Resp<Todo>),
    BuildPaths(Todo, Resp<Todo>),
    EnsurePath(Path, Resp<Todo>),
    AddTempRoot(Path, Resp<Todo>),
    AddIndirectRoot(Todo, Resp<Todo>),
    SyncWithGC(Todo, Resp<Todo>),
    FindRoots(Todo, Resp<Todo>),
    SetOptions(SetOptions, Resp<()>),
    CollectGarbage(Todo, Resp<Todo>),
    QuerySubstitutablePathInfo(Todo, Resp<Todo>),
    QueryAllValidPaths(Todo, Resp<Todo>),
    QueryFailedPaths(Todo, Resp<Todo>),
    ClearFailedPaths(Todo, Resp<Todo>),
    QueryPathInfo(Path, Resp<QueryPathInfoResponse>),
    QueryPathFromHashPart(Todo, Resp<Todo>),
    QuerySubstitutablePathInfos(Todo, Resp<Todo>),
    QueryValidPaths(Todo, Resp<Todo>),
    QuerySubstitutablePaths(Todo, Resp<Todo>),
    QueryValidDerivers(Todo, Resp<Todo>),
    OptimiseStore(Todo, Resp<Todo>),
    VerifyStore(Todo, Resp<Todo>),
    BuildDerivation(Todo, Resp<Todo>),
    AddSignatures(Todo, Resp<Todo>),
    NarFromPath(Todo, Resp<Todo>),
    AddToStoreNar(Todo, Resp<Todo>),
    QueryMissing(QueryMissing, Resp<Todo>),
    QueryDerivationOutputMap(Todo, Resp<Todo>),
    RegisterDrvOutput(Todo, Resp<Todo>),
    QueryRealisation(Todo, Resp<Todo>),
    AddMultipleToStore(Todo, Resp<Todo>),
    AddBuildLog(Todo, Resp<Todo>),
    BuildPathsWithResults(BuildPathsWithResults, Resp<Todo>),
}

#[derive(Debug)]
pub enum StderrMsgOpcode {
    Write = 0x64617416,
    // Read = 0x64617461,
    Error = 0x63787470,
    Next = 0x6f6c6d67,
    StartActivity = 0x53545254,
    StopActivity = 0x53544f50,
    Result = 0x52534c54,
    Last = 0x616c7473,
}

#[derive(Debug)]
pub enum StderrMsg {
    Write(ByteBuf),
    // Read(),
    Error(Todo),
    Next(Todo),
    StartActivity(Todo),
    StopActivity(Todo),
    Result(Todo),
    Last(()),
}

const WORKER_MAGIC_1: u64 = 0x6e697863;
const WORKER_MAGIC_2: u64 = 0x6478696f;
const PROTOCOL_VERSION: DaemonVersion = DaemonVersion {
    major: 1,
    minor: 34,
};
const LVL_ERROR: u64 = 0;

/// Signals that the daemon can send to the client.
pub enum StderrSignal {
    Next = 0x6f6c6d67,
    Read = 0x64617461,  // data needed from source
    Write = 0x64617416, // data for sink
    Last = 0x616c7473,
    Error = 0x63787470,
    StartActivity = 0x53545254,
    StopActivity = 0x53544f50,
    Result = 0x52534c54,
}

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
        let mut deserializer = Deserializer {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorePathSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    paths: Vec<Path>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StringSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    paths: Vec<ByteBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NarHash {
    data: ByteBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidPathInfo {
    deriver: Path, // Can be empty
    hash: NarHash,
    references: StorePathSet,
    registration_time: u64, // In seconds, since the epoch
    nar_size: u64,
    ultimate: bool,
    sigs: StringSet,
    content_address: ByteBuf, // Can be empty
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidPathInfoWithPath {
    path: Path,
    info: ValidPathInfo,
}

#[derive(Clone, Default)]
pub struct FramedData {
    data: Vec<ByteBuf>,
}

impl std::fmt::Debug for FramedData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FramedData").finish_non_exhaustive()
    }
}

pub fn write_worker_op<W: Write>(op: &WorkerOp, mut write: W) -> Result<()> {
    let mut ser = Serializer { write: &mut write };
    macro_rules! op {
            ($($name:ident),*) => {
                match op {
                    $(WorkerOp::$name(inner, _resp) => {
                        (WorkerOpCode::$name as u64).serialize(&mut ser)?;
                        inner.serialize(&mut ser)?;
                    },)*
                }
            };
        }
    op!(
        IsValidPath,
        HasSubstitutes,
        QueryReferrers,
        AddToStore,
        BuildPaths,
        EnsurePath,
        AddTempRoot,
        AddIndirectRoot,
        SyncWithGC,
        FindRoots,
        SetOptions,
        CollectGarbage,
        QuerySubstitutablePathInfo,
        QueryAllValidPaths,
        QueryFailedPaths,
        ClearFailedPaths,
        QueryPathInfo,
        QueryPathFromHashPart,
        QuerySubstitutablePathInfos,
        QueryValidPaths,
        QuerySubstitutablePaths,
        QueryValidDerivers,
        OptimiseStore,
        VerifyStore,
        BuildDerivation,
        AddSignatures,
        NarFromPath,
        AddToStoreNar,
        QueryMissing,
        QueryDerivationOutputMap,
        RegisterDrvOutput,
        QueryRealisation,
        AddMultipleToStore,
        AddBuildLog,
        BuildPathsWithResults
    );
    // TODO: This is horrible
    if let WorkerOp::AddToStore(add, _resp) = op {
        for data in &add.framed.data {
            (data.len() as u64).serialize(&mut ser)?;
            ser.write.write_all(data)?;
        }
        (0u64).serialize(&mut ser)?;
    }
    Ok(())
}

impl<R: Read> NixStoreRead<R> {
    pub fn read_worker_op(&mut self, opcode: WorkerOpCode) -> Result<WorkerOp> {
        macro_rules! op {
            ($($name:ident),*) => {
                match opcode {
                    $(WorkerOpCode::$name => Ok(WorkerOp::$name(serialize::deserialize(&mut self.inner)?, Resp::new()))),*,
                    op => { Err(anyhow!("unknown op code {op:?}")) }
                }
            };
        }
        let op = op!(
            IsValidPath,
            HasSubstitutes,
            QueryReferrers,
            AddToStore,
            BuildPaths,
            EnsurePath,
            AddTempRoot,
            AddIndirectRoot,
            SyncWithGC,
            FindRoots,
            SetOptions,
            CollectGarbage,
            QuerySubstitutablePathInfo,
            QueryAllValidPaths,
            QueryFailedPaths,
            ClearFailedPaths,
            QueryPathInfo,
            QueryPathFromHashPart,
            QuerySubstitutablePathInfos,
            QueryValidPaths,
            QuerySubstitutablePaths,
            QueryValidDerivers,
            OptimiseStore,
            VerifyStore,
            BuildDerivation,
            AddSignatures,
            NarFromPath,
            AddToStoreNar,
            QueryMissing,
            QueryDerivationOutputMap,
            RegisterDrvOutput,
            QueryRealisation,
            AddMultipleToStore,
            AddBuildLog,
            BuildPathsWithResults
        )?;

        if let WorkerOp::AddToStore(mut add, _) = op {
            add.framed = self.read_framed_data()?;
            Ok(WorkerOp::AddToStore(add, Resp::new()))
        } else {
            Ok(op)
        }
    }

    pub fn read_u64(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.inner.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn read_framed_data(&mut self) -> Result<FramedData> {
        let mut ret = FramedData::default();
        loop {
            let len = self.read_u64()?;
            if len == 0 {
                break;
            }
            let mut buf = vec![0; len as usize];
            self.inner.read_exact(&mut buf)?;
            // NOTE: the buffers in framed data are *not* padded.
            ret.data.push(ByteBuf::from(buf));
        }
        Ok(ret)
    }

    fn read_command(&mut self) -> Result<WorkerOp> {
        let op = self.read_u64()?;
        eprintln!("opcode {op:x}");
        let Some(op) = WorkerOpCode::from_u64(op) else {
            todo!("handle bad worker op");
        };
        self.read_worker_op(op)
    }
}

impl<W: Write> NixStoreWrite<W> {
    fn write_u64(&mut self, n: u64) -> Result<()> {
        self.inner.write_all(&n.to_le_bytes())?;
        Ok(())
    }

    fn write_string(&mut self, s: &[u8]) -> Result<()> {
        self.write_u64(s.len() as _)?;
        self.inner.write_all(&s)?;

        if s.len() % 8 > 0 {
            let padding = 8 - s.len() % 8;
            let pad_buf = [0; 8];
            self.inner.write_all(&pad_buf[..padding])?;
        }

        Ok(())
    }

    fn write_framed_data(&mut self, framed: &FramedData) -> Result<()> {
        for data in &framed.data {
            self.write_u64(data.len() as u64)?;
            self.inner.write_all(data)?;
        }
        self.write_u64(0)?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(self.inner.flush()?)
    }
}

impl<R: Read, W: Write> NixReadWrite<R, W> {
    /// Process a remote nix connection.
    /// Reimplement Daemon::processConnection from nix/src/libstore/daemon.cc
    pub fn process_connection(&mut self, proxy_to_nix: bool) -> Result<()>
    where
        W: Send,
    {
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

        if client_version < 0x10a {
            eprintln!("Client version {client_version} is too old");
            todo!("handle error: client version");
        }

        // TODO keep track of number of WorkerOps performed
        let mut _op_count: u64 = 0;

        let daemon_version = DaemonVersion::from(client_version);

        if daemon_version.minor >= 14 {
            let _obsolete_cpu_affinity = self.read.read_u64()?;
        }

        if daemon_version.minor >= 11 {
            let _obsolete_reserve_space = self.read.read_u64()?;
        }

        if daemon_version.minor >= 33 {
            // TODO figure out what we need to set as the version
            self.write.write_string("rust-nix-bazel-0.1.0".as_bytes())?;
        }
        self.write.write_u64(StderrSignal::Last as u64)?; // Send startup messages to the client
        self.write.flush()?;

        if proxy_to_nix {
            self.proxy.write_u64(WORKER_MAGIC_1)?;
            self.proxy.flush()?;
            if self.proxy.read_u64()? != WORKER_MAGIC_2 {
                todo!("Handle proxy daemon protocol mismatch");
            }
            if self.proxy.read_u64()? != PROTOCOL_VERSION.into() {
                todo!("Handle proxy daemon protocol version mismatch");
            }
            self.proxy.write_u64(client_version)?;
            self.proxy.write_u64(0)?; // cpu affinity
            self.proxy.write_u64(0)?; // reserve space
            self.proxy.flush()?;
            let proxy_daemon_version = self.proxy.read_string()?;
            eprintln!(
                "Proxy daemon is: {}",
                String::from_utf8_lossy(proxy_daemon_version.as_ref())
            );
            if self.proxy.read_u64()? != StderrSignal::Last as u64 {
                todo!("Drain stderr");
            }
        }

        std::thread::scope(|scope| {
            let write = &mut self.write.inner;
            let read = &mut self.proxy.child_out;
            scope.spawn(|| -> Result<()> {
                loop {
                    let mut buf = [0u8; 1024];
                    let read_bytes = read.read(&mut buf).unwrap();
                    write.write_all(&buf[..read_bytes]).unwrap();
                    write.flush().unwrap();
                }
            });

            loop {
                /*
                let mut buf = [0u8; 1024];
                let read_bytes = self.read.inner.read(&mut buf).unwrap();
                if read_bytes > 0 {
                    eprintln!("send bytes {:?}", &buf[..read_bytes]);
                }
                self.proxy.child_in.write_all(&buf[..read_bytes]).unwrap();
                self.proxy.child_in.flush().unwrap();
                */
                let mut read = NixStoreRead {
                    inner: printing_read::PrintingRead {
                        buf: Vec::new(),
                        inner: &mut self.read.inner,
                    },
                };

                // TODO process worker ops
                let op = match read.read_command() {
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                        eprintln!("EOF, closing");
                        break;
                    }
                    x => x,
                }?;

                let mut buf = Vec::new();
                write_worker_op(&op, &mut buf).unwrap();
                if buf != read.inner.buf {
                    eprintln!("mismatch!");
                    eprintln!("{buf:?}");
                    eprintln!("{:?}", read.inner.buf);
                    panic!();
                }

                eprintln!("read op {op:?}");
                write_worker_op(&op, &mut self.proxy.child_in).unwrap();
                self.proxy.child_in.flush().unwrap();
            }
            Ok(())
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SetOptions {
    pub keep_failing: u64,
    pub keep_going: u64,
    pub try_fallback: u64,
    pub verbosity: u64,
    pub max_build_jobs: u64,
    pub max_silent_time: u64,
    _use_build_hook: u64,
    pub build_verbosity: u64,
    _log_type: u64,
    _print_build_trace: u64,
    pub build_cores: u64,
    pub use_substitutes: u64,
    pub options: Vec<(ByteBuf, ByteBuf)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddToStore {
    name: Path,
    cam_str: Path,
    refs: StorePathSet,
    repair: bool,
    // Note: this can be big, so we will eventually want to stream it.
    #[serde(skip)]
    framed: FramedData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildPathsWithResults {
    paths: Vec<Path>,
    // TODO: make this an enum
    build_mode: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryMissing {
    paths: Vec<Path>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryPathInfoResponse {
    valid: bool,
    path: Option<ValidPathInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Todo {}

impl<'de> Deserialize<'de> for Todo {
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        todo!()
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
mod tests {
    use crate::serialize::Serializer;

    use super::*;

    #[test]
    fn test_serialize() {
        let options = SetOptions {
            keep_failing: 77,
            keep_going: 77,
            try_fallback: 77,
            verbosity: 77,
            max_build_jobs: 77,
            max_silent_time: 77,
            _use_build_hook: 77,
            build_verbosity: 77,
            _log_type: 77,
            _print_build_trace: 77,
            build_cores: 77,
            use_substitutes: 77,
            options: vec![(
                ByteBuf::from(b"buf1".to_owned()),
                ByteBuf::from(b"buf2".to_owned()),
            )],
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut serializer = Serializer { write: &mut cursor };
        options.serialize(&mut serializer).unwrap();

        cursor.set_position(0);
        let mut deserializer = Deserializer { read: &mut cursor };
        assert_eq!(options, SetOptions::deserialize(&mut deserializer).unwrap());
    }
}
