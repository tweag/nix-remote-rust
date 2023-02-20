use anyhow::{anyhow, bail, Error, Result};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{
    de::{DeserializeSeed, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use serde_bytes::ByteBuf;
use std::io::{self, Read, Write};

mod serialize;
use serialize::Deserializer;

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
pub enum WorkerOp {
    IsValidPath(Path),
    HasSubstitutes(()), // TODO
    QueryReferrers(()), // TODO
    AddToStore(AddToStore),
    BuildPaths(()), // TODO
    EnsurePath(Path),
    AddTempRoot(Path),
    AddIndirectRoot(()), // TODO
    SyncWithGC(()),      // TODO
    FindRoots(()),       // TODO
    SetOptions(SetOptions),
    CollectGarbage(()),             // TODO
    QuerySubstitutablePathInfo(()), // TODO
    QueryAllValidPaths(()),         // TODO
    QueryFailedPaths(()),           // TODO
    ClearFailedPaths(()),           // TODO
    QueryPathInfo(Path),
    QueryPathFromHashPart(()),       // TODO
    QuerySubstitutablePathInfos(()), // TODO
    QueryValidPaths(()),             // TODO
    QuerySubstitutablePaths(()),     // TODO
    QueryValidDerivers(()),          // TODO
    OptimiseStore(()),               // TODO
    VerifyStore(()),                 // TODO
    BuildDerivation(()),             // TODO
    AddSignatures(()),               // TODO
    NarFromPath(()),                 // TODO
    AddToStoreNar(()),               // TODO
    QueryMissing(()),                // TODO
    QueryDerivationOutputMap(()),    // TODO
    RegisterDrvOutput(()),           // TODO
    QueryRealisation(()),            // TODO
    AddMultipleToStore(()),          // TODO
    AddBuildLog(()),                 // TODO
    BuildPathsWithResults(()),       // TODO
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
}

pub struct NixReadWrite<R, W> {
    pub read: R,
    pub write: W,
    // pub proxy: NixProxy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorePathSet {
    // TODO: in nix, they call `parseStorePath` to separate store directory from path
    paths: Vec<ByteBuf>,
}

pub struct ValidPathInfo {
    path: Path,
}

#[derive(Clone, Debug, Default, Serialize)] // FIXME: Serialize
pub struct FramedData {
    data: Vec<ByteBuf>,
}

impl<'de> Deserialize<'de> for FramedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visit {}

        impl<'de> Visitor<'de> for Visit {
            type Value = FramedData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("framed data")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut data = Vec::new();
                loop {
                    match seq.next_element::<ByteBuf>()? {
                        Some(elt) if !elt.is_empty() => {
                            data.push(elt);
                        }
                        _ => break,
                    }
                }
                Ok(FramedData { data })
            }
        }

        // When deserializing FramedData *we* want to be in charge of deciding when to stop.
        // Since deserialize_seq reads the length from the stream, we use deserialize_tuple
        // and pass a giant length so that they'll keep giving us data until we stop.
        deserializer.deserialize_tuple(usize::MAX, Visit {})
    }
}

impl ValidPathInfo {
    pub fn write<R: Read, W: Write>(
        &self,
        rw: &mut NixReadWrite<R, W>,
        include_path: bool,
    ) -> Result<()> {
        if include_path {
            rw.write_string(&self.path.0)?;
        }
        rw.write_string(b"")?; // deriver
        rw.write_string(b"0000000000000000000000000000000000000000000000000000000000000000")?; // narhash
        rw.write_u64(0)?; // number of references
                          // write the references here
        rw.write_u64(0)?; // registrationTime
        rw.write_u64(32)?; // narSize
        rw.write_u64(true as u64)?; // ultimate (built locally?)
        rw.write_u64(0)?; // sigs (first is number of strings, which we set to 0)
        rw.write_string(b"")?; // content addressed address (empty string if input addressed)
        Ok(())
    }
}

impl<R: Read, W: Write> NixReadWrite<R, W> {
    pub fn read_worker_op(&mut self, opcode: WorkerOpCode) -> Result<WorkerOp> {
        macro_rules! op {
            ($($name:ident),*) => {
                match opcode {
                    $(WorkerOpCode::$name => Ok(WorkerOp::$name(serialize::deserialize(&mut self.read)?))),*,
                    op => { Err(anyhow!("unknown op code {op:?}")) }
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
        )
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.read.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn read_bool(&mut self) -> Result<bool> {
        self.read_u64().map(|i| i != 0)
    }

    pub fn read_framed_data(&mut self) -> Result<()> {
        loop {
            let len = self.read_u64()?;
            if len == 0 {
                break;
            }
            let mut buf = vec![0; len as usize];
            self.read.read_exact(&mut buf)?;
        }
        Ok(())
    }

    pub fn read_string(&mut self) -> Result<Vec<u8>> {
        // possible errors:
        // Unexecpted EOF
        // IO Error
        // out of memory
        let len = self.read_u64()? as usize;

        // FIXME don't initialize
        let mut buf = vec![0; len];
        self.read.read_exact(&mut buf)?;

        if len % 8 > 0 {
            let padding = 8 - len % 8;
            let mut pad_buf = [0; 8];
            self.read.read_exact(&mut pad_buf[..padding])?;
        }

        Ok(buf)
    }

    pub fn read_store_path_set(&mut self) -> Result<StorePathSet> {
        let len = self.read_u64()?;
        let mut ret = vec![];
        for _ in 0..len {
            ret.push(ByteBuf::from(self.read_string()?));
        }
        Ok(StorePathSet { paths: ret })
    }

    fn write_u64(&mut self, n: u64) -> Result<()> {
        self.write.write(&n.to_le_bytes())?;
        Ok(())
    }

    fn write_string(&mut self, s: &[u8]) -> Result<()> {
        self.write_u64(s.len() as _)?;
        self.write.write_all(&s)?;

        if s.len() % 8 > 0 {
            let padding = 8 - s.len() % 8;
            let pad_buf = [0; 8];
            self.write.write_all(&pad_buf[..padding])?;
        }

        Ok(())
    }

    fn read_command(&mut self) -> Result<WorkerOp> {
        eprintln!("read_command");
        let op = self.read_u64()?;
        eprintln!("op: {op:x}");
        let Some(op) = WorkerOpCode::from_u64(op) else {
            todo!("handle bad worker op");
        };
        self.read_worker_op(op)
    }

    fn reply_command(&mut self, op: WorkerOp) -> Result<()> {
        eprintln!("{op:#?}");
        match op {
            WorkerOp::SetOptions(_) => {}
            WorkerOp::AddTempRoot(_path) => {
                self.write_u64(StderrSignal::Last as u64)?; // Send startup messages to the client
                self.write_u64(1)?;
                self.write.flush()?;
            }
            WorkerOp::IsValidPath(_path) => {
                self.write_u64(StderrSignal::Last as u64)?; // Send startup messages to the client
                self.write_u64(true as u64)?;
                self.write.flush()?;
            }
            WorkerOp::AddToStore(add_to_store) => {
                self.write_u64(StderrSignal::Last as u64)?; // Send startup messages to client
                ValidPathInfo {
                    path: add_to_store.name,
                }
                .write(self, true)?;
                self.write.flush()?;
            }
            WorkerOp::QueryPathInfo(path) => {
                self.write_u64(StderrSignal::Last as u64)?; // Send startup messages to the client
                self.write_u64(1)?;
                ValidPathInfo { path }.write(self, false)?;
                self.write.flush()?;
            }
            _ => bail!("We don't know what to do"),
        }
        Ok(())
    }

    /// Process a remote nix connection.
    /// Reimplement Daemon::processConnection from nix/src/libstore/daemon.cc
    pub fn process_connection(&mut self, proxy_to_nix: bool) -> Result<()> {
        let magic = self.read_u64()?;
        if magic != WORKER_MAGIC_1 {
            eprintln!("{magic:x}");
            eprintln!("{WORKER_MAGIC_1:x}");
            todo!("handle error: protocol mismatch 1");
        }

        self.write_u64(WORKER_MAGIC_2)?;
        self.write_u64(PROTOCOL_VERSION.into())?;
        self.write.flush()?;

        let client_version = self.read_u64()?;

        if client_version < 0x10a {
            eprintln!("Client version {client_version} is too old");
            todo!("handle error: client version");
        }

        // TODO keep track of number of WorkerOps performed
        let mut _op_count: u64 = 0;

        let daemon_version = DaemonVersion::from(client_version);

        if daemon_version.minor >= 14 {
            let _obsolete_cpu_affinity = self.read_u64()?;
        }

        if daemon_version.minor >= 11 {
            let _obsolete_reserve_space = self.read_u64()?;
        }

        if daemon_version.minor >= 33 {
            // TODO figure out what we need to set as the version
            self.write_string("rust-nix-bazel-0.1.0".as_bytes())?;
        }
        self.write_u64(StderrSignal::Last as u64)?; // Send startup messages to the client
        self.write.flush()?;

        loop {
            // TODO process worker ops
            let op = self.read_command()?;
            self.reply_command(op)?;
        }
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
    cam_str: ByteBuf,
    refs: StorePathSet,
    repair: bool,
    // Note: this can be big, so we will eventually want to stream it.
    framed: FramedData,
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
