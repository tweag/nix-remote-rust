use anyhow::{anyhow, bail};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{self, Read, Write};

use crate::{
    serialize::{Deserializer, Serializer},
    FramedSource, NarHash, Path, Result, StorePathSet, StringSet,
};

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

/// The different worker ops.
///
/// On the wire, they are represented as the opcode followed by the body.
///
/// TODO: It would be neat if we could just derive the serialize/deserialize
/// implementations, since this is a common pattern.
/// We'd like to write this definition like:
///
/// ```ignore
/// pub enum WorkerOp {
///    #[nix_enum(tag = 1)]
///    IsValidPath(Path, Resp<bool>),
///    #[nix_enum(tag = 2)]
///    HasSubstitutes(Todo, Resp<Todo>),
/// // ...
/// }
/// ```
///
/// and then just get rid of the Opcode enum above.
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

impl WorkerOp {
    /// Reads a worker op from the wire protocol.
    pub fn read(mut r: impl Read) -> Result<Self> {
        let mut de = Deserializer { read: &mut r };
        let opcode = u64::deserialize(&mut de)?;
        let opcode = WorkerOpCode::from_u64(opcode)
            .ok_or_else(|| anyhow!("invalid worker op code {opcode}"))?;

        macro_rules! op {
            ($($name:ident),*) => {
                match opcode {
                    $(WorkerOpCode::$name => Ok(WorkerOp::$name(<_>::deserialize(&mut de)?, Resp::new()))),*,
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

        //
        if let WorkerOp::AddToStore(mut add, _) = op {
            add.framed = FramedSource::read(&mut r)?;
            Ok(WorkerOp::AddToStore(add, Resp::new()))
        } else {
            Ok(op)
        }
    }

    pub fn write(&self, mut write: impl Write) -> Result<()> {
        let mut ser = Serializer { write: &mut write };
        macro_rules! op {
            ($($name:ident),*) => {
                match self {
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
        if let WorkerOp::AddToStore(add, _resp) = self {
            add.framed.write(write)?;
        }
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
    framed: FramedSource,
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

#[cfg(test)]
mod tests {
    use crate::{serialize::Serializer, worker_op::SetOptions};

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
