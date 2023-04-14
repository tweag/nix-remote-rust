//! Nix propagates error messages from the daemon to the client using the following pattern:
//! - the daemon reads a worker op from the client,
//! - the daemon sends one or more stderr messages to the client. Each message consists of
//!   a 64-bit opcode followed by the body of the message. The final message has the opcode `Last`.
//! - the daemon sends the reply to the worker op.

use anyhow::anyhow;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::de::{Error, SeqAccess};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{Read, Write};

use crate::serialize::{Deserializer, Serializer};
use crate::{NixString, Result};

/// The different opcodes. In the nix source, they are named like STDERR_WRITE, STDERR_START_ACTIVITY, etc.
#[derive(Debug, FromPrimitive)]
pub enum Opcode {
    Write = 0x64617416,
    // STDERR_READ is... interesting. Fortunately, it appears to have been superceded by FramedSource.
    // It is not used in the current version of the nix protocol
    // Read = 0x64617461,
    Error = 0x63787470,
    Next = 0x6f6c6d67,
    StartActivity = 0x53545254,
    StopActivity = 0x53544f50,
    Result = 0x52534c54,
    Last = 0x616c7473,
}

/// The different stderr messages.
///
/// On the wire, they are represented as the opcode followed by the body.
///
/// TODO: It would be neat if we could just derive the serialize/deserialize
/// implementations, since this is a common pattern.
#[derive(Debug)]
pub enum Msg {
    Write(NixString),
    Error(StderrError),
    Next(NixString),
    StartActivity(StderrStartActivity),
    StopActivity(u64),
    Result(StderrResult),
    Last(()),
}

impl Msg {
    /// Write this message out in its wire format.
    pub fn write(&self, mut write: impl Write) -> Result<()> {
        let mut ser = Serializer { write: &mut write };
        macro_rules! msg {
            ($($name:ident),*) => {
                match self {
                    $(Msg::$name(inner) => {
                        (Opcode::$name as u64).serialize(&mut ser)?;
                        inner.serialize(&mut ser)?;
                    },)*
                }
            };
        }
        msg!(
            Write,
            Error,
            Next,
            StartActivity,
            StopActivity,
            Result,
            Last
        );
        Ok(())
    }

    /// Read a message from its wire representation.
    pub fn read(mut read: impl Read) -> Result<Self> {
        let mut deser = Deserializer { read: &mut read };

        let opcode = u64::deserialize(&mut deser)?;
        let opcode =
            Opcode::from_u64(opcode).ok_or_else(|| anyhow!("invalid stderr op code {opcode:x}"))?;

        macro_rules! msg {
            ($($name:ident),*) => {
                match opcode {
                    $(Opcode::$name => Ok(Msg::$name(<_>::deserialize(&mut deser)?))),*,
                }
            };
        }

        msg!(
            Write,
            Error,
            Next,
            StartActivity,
            StopActivity,
            Result,
            Last
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StderrError {
    typ: ByteBuf,
    level: u64,
    name: ByteBuf,
    message: ByteBuf,
    have_pos: u64,
    traces: Vec<Trace>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StderrStartActivity {
    act: u64,
    lvl: u64,
    typ: u64,
    s: ByteBuf,
    fields: LoggerFields,
    parent: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StderrResult {
    act: u64,
    typ: u64,
    fields: LoggerFields,
}

#[derive(Debug, Deserialize, Serialize)]
struct Trace {
    have_pos: u64,
    trace: ByteBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct LoggerFields {
    fields: Vec<LoggerField>,
}

#[derive(Debug, Serialize)]
enum LoggerField {
    #[serde(serialize_with = "serialize_logger_u64")]
    Int(u64),
    #[serde(serialize_with = "serialize_logger_string")]
    String(ByteBuf),
}

pub fn serialize_logger_u64<S>(field_number: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    (0u64, field_number).serialize(serializer)
}

pub fn serialize_logger_string<S>(
    field_string: &serde_bytes::ByteBuf,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    (1u64, field_string).serialize(serializer)
}

impl<'de> Deserialize<'de> for LoggerField {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'d> serde::de::Visitor<'d> for Visitor {
            type Value = LoggerField;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("either a string or an int")
            }

            fn visit_seq<A: SeqAccess<'d>>(self, mut seq: A) -> Result<LoggerField, A::Error> {
                let tag: u64 = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("failed to read logger field tag"))?;
                match tag {
                    0 => {
                        let val: u64 = seq
                            .next_element()?
                            .ok_or_else(|| A::Error::custom("failed to read logger field int"))?;
                        Ok(LoggerField::Int(val))
                    }
                    1 => {
                        let val: ByteBuf = seq.next_element()?.ok_or_else(|| {
                            A::Error::custom("failed to read logger field string")
                        })?;
                        Ok(LoggerField::String(val))
                    }
                    _ => Err(A::Error::custom("unknown logger field type")),
                }
            }
        }

        deserializer.deserialize_tuple(2, Visitor)
    }
}
