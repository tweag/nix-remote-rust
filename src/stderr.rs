//! Log messages from the nix daemon.
//!
//! Nix propagates error messages from the daemon to the client using the following pattern:
//! - the daemon reads a worker op from the client,
//! - the daemon sends one or more stderr messages to the client. Each message consists of
//!   a 64-bit opcode followed by the body of the message. The final message has the opcode `Last`.
//! - the daemon sends the reply to the worker op.

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use tagged_serde::TaggedSerde;

use crate::{NixString, Result};

/// The different stderr messages.
///
/// On the wire, they are represented as the opcode followed by the body.
///
// STDERR_READ is... interesting. Fortunately, it appears to have been superceded by FramedSource.
// It is not used in the current version of the nix protocol
// Read = 0x64617461,
#[derive(Debug, TaggedSerde, PartialEq, Clone, Eq)]
pub enum Msg {
    #[tagged_serde = 0x64617416]
    Write(NixString),
    #[tagged_serde = 0x63787470]
    Error(StderrError),
    #[tagged_serde = 0x6f6c6d67]
    Next(NixString),
    #[tagged_serde = 0x53545254]
    StartActivity(StderrStartActivity),
    #[tagged_serde = 0x53544f50]
    StopActivity(u64),
    #[tagged_serde = 0x52534c54]
    Result(StderrResult),
    #[tagged_serde = 0x616c7473]
    Last(()),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct StderrError {
    typ: String,
    level: u64,
    name: String,
    message: String,
    have_pos: u64,
    traces: Vec<Trace>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct StderrStartActivity {
    act: u64,
    lvl: u64,
    typ: u64,
    s: String,
    fields: LoggerFields,
    parent: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct StderrResult {
    act: u64,
    typ: u64,
    fields: LoggerFields,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
struct Trace {
    have_pos: u64,
    trace: ByteBuf,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
struct LoggerFields {
    fields: Vec<LoggerField>,
}

#[derive(Debug, TaggedSerde, Clone, PartialEq, Eq)]
enum LoggerField {
    #[tagged_serde = 0]
    Int(u64),
    #[tagged_serde = 1]
    String(String),
}
