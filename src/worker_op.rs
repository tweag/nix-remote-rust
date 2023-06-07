use anyhow::anyhow;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{Read, Write};
use tagged_serde::TaggedSerde;

use crate::PathSet;
use crate::{
    serialize::{NixDeserializer, NixReadExt, NixSerializer, NixWriteExt},
    FramedData, NarHash, NixString, Path, Result, StorePathSet, StringSet, ValidPathInfoWithPath,
};

/// A zero-sized marker type. Its job is to mark the expected response
/// type for each worker op.
#[derive(Debug, Serialize, Deserialize)]
pub struct Resp<T> {
    #[serde(skip)]
    marker: std::marker::PhantomData<T>,
}

impl<T> Resp<T> {
    fn new() -> Resp<T> {
        Resp {
            marker: std::marker::PhantomData,
        }
    }

    fn ty(&self, v: T) -> T {
        v
    }
}

/// The different worker ops.
///
/// On the wire, they are represented as the opcode followed by the body.
///
/// The second argument in each variant is a tag denoting the expected return value.
#[derive(Debug, TaggedSerde)]
pub enum WorkerOp {
    #[tagged_serde = 1]
    IsValidPath(Path, Resp<bool>),
    #[tagged_serde = 3]
    HasSubstitutes(Todo, Resp<Todo>),
    #[tagged_serde = 6]
    QueryReferrers(Todo, Resp<Todo>),
    #[tagged_serde = 7]
    AddToStore(AddToStore, Resp<ValidPathInfoWithPath>),
    #[tagged_serde = 9]
    BuildPaths(BuildPaths, Resp<u64>),
    #[tagged_serde = 10]
    EnsurePath(Path, Resp<u64>),
    #[tagged_serde = 11]
    AddTempRoot(Path, Resp<u64>),
    #[tagged_serde = 12]
    AddIndirectRoot(Todo, Resp<Todo>),
    #[tagged_serde = 13]
    SyncWithGC(Todo, Resp<Todo>),
    #[tagged_serde = 14]
    FindRoots(Todo, Resp<Todo>),
    #[tagged_serde = 19]
    SetOptions(SetOptions, Resp<()>),
    #[tagged_serde = 20]
    CollectGarbage(CollectGarbage, Resp<CollectGarbageResponse>),
    #[tagged_serde = 21]
    QuerySubstitutablePathInfo(Todo, Resp<Todo>),
    #[tagged_serde = 23]
    QueryAllValidPaths(Todo, Resp<Todo>),
    #[tagged_serde = 24]
    QueryFailedPaths(Todo, Resp<Todo>),
    #[tagged_serde = 25]
    ClearFailedPaths(Todo, Resp<Todo>),
    #[tagged_serde = 26]
    QueryPathInfo(Path, Resp<QueryPathInfoResponse>),
    #[tagged_serde = 29]
    QueryPathFromHashPart(Todo, Resp<Todo>),
    #[tagged_serde = 30]
    QuerySubstitutablePathInfos(Todo, Resp<Todo>),
    #[tagged_serde = 31]
    QueryValidPaths(Todo, Resp<Todo>),
    #[tagged_serde = 32]
    QuerySubstitutablePaths(Todo, Resp<Todo>),
    #[tagged_serde = 33]
    QueryValidDerivers(Todo, Resp<Todo>),
    #[tagged_serde = 34]
    OptimiseStore(Todo, Resp<Todo>),
    #[tagged_serde = 35]
    VerifyStore(Todo, Resp<Todo>),
    #[tagged_serde = 36]
    BuildDerivation(Todo, Resp<Todo>),
    #[tagged_serde = 37]
    AddSignatures(Todo, Resp<Todo>),
    #[tagged_serde = 38]
    NarFromPath(Path, Resp<Nar>),
    #[tagged_serde = 39]
    AddToStoreNar(Todo, Resp<Todo>),
    #[tagged_serde = 40]
    QueryMissing(QueryMissing, Resp<QueryMissingResponse>),
    #[tagged_serde = 41]
    QueryDerivationOutputMap(Path, Resp<DerivationOutputMap>),
    #[tagged_serde = 42]
    RegisterDrvOutput(Todo, Resp<Todo>),
    #[tagged_serde = 43]
    QueryRealisation(Todo, Resp<Todo>),
    #[tagged_serde = 44]
    AddMultipleToStore(Todo, Resp<Todo>),
    #[tagged_serde = 45]
    AddBuildLog(Todo, Resp<Todo>),
    #[tagged_serde = 46]
    BuildPathsWithResults(BuildPaths, Resp<Vec<BuildResult>>),
}

macro_rules! for_each_op {
    ($macro_name:ident !) => {
        $macro_name!(
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
    };
}

impl WorkerOp {
    /// Reads a worker op from the wire protocol.
    pub fn read(mut r: impl Read) -> Result<Self> {
        let op: WorkerOp = r.read_nix()?;

        // After reading AddToStore, Nix reads from a FramedSource. Since we're
        // temporarily putting the FramedSource in the AddToStore, read it here.
        //
        // This will also need to be handled in AddMultipleToStore, AddToStoreNar,
        // and AddBuildLog.
        if let WorkerOp::AddToStore(mut add, _) = op {
            add.framed = FramedData::read(&mut r)?;
            Ok(WorkerOp::AddToStore(add, Resp::new()))
        } else {
            Ok(op)
        }
    }

    pub fn write(&self, mut write: impl Write) -> Result<()> {
        write.write_nix(self)?;

        // See the comment in WorkerOp::read
        if let WorkerOp::AddToStore(add, _resp) = self {
            add.framed.write(write)?;
        }
        Ok(())
    }

    pub fn proxy_response(&self, mut read: impl Read, mut write: impl Write) -> Result<()> {
        let mut logging_read = crate::printing_read::PrintingRead {
            buf: Vec::new(),
            inner: &mut read,
        };
        let mut deser = NixDeserializer {
            read: &mut logging_read,
        };
        let mut ser = NixSerializer { write: &mut write };
        let mut dbg_buf = Vec::new();
        let mut dbg_ser = NixSerializer {
            write: &mut dbg_buf,
        };
        macro_rules! respond {
            ($($name:ident),*) => {
                match self {
                    $(WorkerOp::$name(_inner, resp) => {
                        let reply = resp.ty(<_>::deserialize(&mut deser)?);
                        eprintln!("read reply {reply:?}");

                        reply.serialize(&mut dbg_ser)?;
                        if dbg_buf != logging_read.buf {
                            eprintln!("mismatch!");
                            eprintln!("{dbg_buf:?}");
                            eprintln!("{:?}", logging_read.buf);
                            panic!();
                        }
                        reply.serialize(&mut ser)?;
                    },)*
                }
            };
        }

        for_each_op!(respond!);
        Ok(())
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
    pub options: Vec<(NixString, NixString)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddToStore {
    name: Path,
    cam_str: Path,
    refs: StorePathSet,
    repair: bool,
    // TODO: This doesn't really belong here. It shouldn't be read as part of a
    // worker op: it should really be streamed.
    #[serde(skip)]
    framed: FramedData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildPaths {
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
    path: Option<ValidPathInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryMissingResponse {
    will_build: StorePathSet,
    will_substitute: StorePathSet,
    unknown: StorePathSet,
    download_size: u64,
    nar_size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildResult {
    path: NixString,
    status: u64,
    error_msg: NixString,
    time_built: u64,
    is_non_deterministic: u64,
    start_time: u64,
    stop_time: u64,
    built_outputs: DrvOutputs,
}

// TODO: first NixString is a DrvOutput; second is a Realisation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DrvOutputs(Vec<(NixString, NixString)>);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectGarbage {
    action: GcAction,
    paths_to_delete: StorePathSet,
    ignore_liveness: bool,
    max_freed: u64,
    _obsolete0: u64,
    _obsolete1: u64,
    _obsolete2: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DerivationOutputMap {
    paths: Vec<(NixString, Path)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectGarbageResponse {
    paths: PathSet,
    bytes_freed: u64,
    _obsolete: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[repr(u64)]
#[serde(try_from = "u64")]
#[serde(into = "u64")]
pub enum GcAction {
    ReturnLive = 0,
    ReturnDead = 1,
    #[default]
    DeleteDead = 2,
    DeleteSpecific = 3,
}

impl TryFrom<u64> for GcAction {
    type Error = &'static str;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(GcAction::ReturnLive),
            1 => Ok(GcAction::ReturnDead),
            2 => Ok(GcAction::DeleteDead),
            3 => Ok(GcAction::DeleteSpecific),
            _ => Err("wrong number"),
        }
    }
}

impl From<GcAction> for u64 {
    fn from(value: GcAction) -> Self {
        value as _
    }
}

/// A struct that panics when attempting to deserialize it. For marking
/// parts of the protocol that we haven't implemented yet.
#[derive(Debug, Clone, Serialize)]
pub struct Todo {}

impl<'de> Deserialize<'de> for Todo {
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(D::Error::unknown_variant("unknown", &[]))
    }
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

pub struct Nar {
    entries: Vec<NarEntry>,
}

pub enum NarEntry {
    Type(NarType),
    Contents {
        contents: NixString,
        executable: bool,
    },
    Target(NixString),
    Directory(Vec<NarDirectoryEntry>),
}

pub enum NarType {
    Regular,
    Directory,
    Symlink,
}

pub enum NarDirectoryEntry {
    Name(NixString),
    Node(Nar),
}

#[cfg(test)]
mod tests {
    use crate::{serialize::NixSerializer, worker_op::SetOptions};

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
                NixString(ByteBuf::from(b"buf1".to_owned())),
                NixString(ByteBuf::from(b"buf2".to_owned())),
            )],
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut serializer = NixSerializer { write: &mut cursor };
        options.serialize(&mut serializer).unwrap();

        cursor.set_position(0);
        let mut deserializer = NixDeserializer { read: &mut cursor };
        assert_eq!(options, SetOptions::deserialize(&mut deserializer).unwrap());
    }
}
