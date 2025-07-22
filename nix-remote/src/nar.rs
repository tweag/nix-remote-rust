//! The Nar archive format.
//!
//! The [`Nar`] struct represents a nar archive (essentially a directory tree) in memory.
//! Since these can be large, it is often preferred to avoid buffering an entire nar in
//! memory; the `stream` function allows for streaming a `Nar` (represented in the nix wire
//! format) from a `std::io::Read` to a `std::io::Write`.

use serde::{de::SeqAccess, ser::SerializeTuple, Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::{
    serialize::{NixDeserializer, Tee},
    NixString,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct NarFile {
    pub contents: NixString,
    pub executable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub enum Nar {
    Contents(NarFile),
    Target(NixString),
    Directory(Vec<NarDirectoryEntry>),
}

impl Default for Nar {
    fn default() -> Nar {
        Nar::Contents(NarFile::default())
    }
}

impl Nar {
    /// Recursively sort all directories by name.
    pub fn sort(&mut self) {
        if let Nar::Directory(entries) = self {
            entries.sort_by(|a, b| a.name.cmp(&b.name));

            for e in entries {
                e.node.sort();
            }
        }
    }
}

// TODO: if tagged_serde supported tagging with arbitrary ser/de types,
// we could use it here
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct NarDirectoryEntry {
    pub name: NixString,
    pub node: Nar,
}

pub trait EntrySink<'a>: 'a {
    type DirectorySink: DirectorySink<'a>;
    type FileSink: FileSink;

    fn become_directory(self) -> Self::DirectorySink;
    fn become_file(self) -> Self::FileSink;
    fn become_symlink(self, target: NixString);
}

// The workaround for
// https://github.com/rust-lang/rust/issues/87479
pub trait DirectorySinkSuper {
    type EntrySink<'b>: EntrySink<'b>;
}

pub trait DirectorySink<'a>: DirectorySinkSuper {
    fn create_entry<'b>(&'b mut self, name: NixString) -> Self::EntrySink<'b>
    where
        'a: 'b;
}

pub trait FileSink: std::io::Write {
    fn set_executable(&mut self, executable: bool);
    fn add_contents(&mut self, contents: &[u8]);
}

impl<'a> EntrySink<'a> for &'a mut Nar {
    type DirectorySink = &'a mut Vec<NarDirectoryEntry>;
    type FileSink = &'a mut NarFile;

    fn become_directory(self) -> Self::DirectorySink {
        *self = Nar::Directory(Vec::new());
        let Nar::Directory(dir) = self else {
            unreachable!()
        };
        dir
    }

    fn become_file(self) -> Self::FileSink {
        *self = Nar::Contents(NarFile {
            executable: false,
            contents: NixString::default(),
        });
        // TODO: can we express this better?
        let Nar::Contents(contents) = self else {
            unreachable!()
        };
        contents
    }

    fn become_symlink(self, target: NixString) {
        *self = Nar::Target(target);
    }
}

impl<'a> DirectorySinkSuper for &'a mut Vec<NarDirectoryEntry> {
    type EntrySink<'b> = &'b mut Nar;
}

impl<'a> DirectorySink<'a> for &'a mut Vec<NarDirectoryEntry> {
    fn create_entry<'b>(&'b mut self, name: NixString) -> Self::EntrySink<'b>
    where
        'a: 'b,
    {
        self.push(NarDirectoryEntry {
            name,
            node: Nar::Contents(NarFile {
                contents: NixString::default(),
                executable: false,
            }),
        });
        &mut self.last_mut().unwrap().node
    }
}

impl<'a> std::io::Write for &'a mut NarFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.add_contents(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> FileSink for &'a mut NarFile {
    fn set_executable(&mut self, executable: bool) {
        self.executable = executable;
    }

    fn add_contents(&mut self, contents: &[u8]) {
        self.contents.0.extend_from_slice(contents);
    }
}

#[derive(Default)]
struct Null;

impl<'a> std::io::Write for &'a mut Null {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> FileSink for &'a mut Null {
    fn set_executable(&mut self, _executable: bool) {}

    fn add_contents(&mut self, _contents: &[u8]) {}
}

impl<'a> EntrySink<'a> for &'a mut Null {
    type DirectorySink = &'a mut Null;
    type FileSink = &'a mut Null;

    fn become_directory(self) -> Self::DirectorySink {
        self
    }

    fn become_file(self) -> Self::FileSink {
        self
    }

    fn become_symlink(self, _target: NixString) {}
}

impl<'a> DirectorySinkSuper for &'a mut Null {
    type EntrySink<'b> = &'b mut Null;
}

impl<'a> DirectorySink<'a> for &'a mut Null {
    fn create_entry<'b>(&'b mut self, _name: NixString) -> Self::EntrySink<'b>
    where
        'a: 'b,
    {
        self
    }
}

trait SerializeTupleExt: SerializeTuple {
    fn serialize_buf(&mut self, s: impl AsRef<[u8]>) -> Result<(), Self::Error> {
        self.serialize_element(&ByteBuf::from(s.as_ref()))
    }
}

impl<S: SerializeTuple> SerializeTupleExt for S {}

// A trait that lets you read strings one-by-one in the Nix wire format.
trait StringReader<'a> {
    type Error: serde::de::Error;

    fn expect_string(&mut self) -> Result<NixString, Self::Error>;

    fn expect_tag(&mut self, s: &str) -> Result<(), Self::Error> {
        let tag = self.expect_string()?;
        if tag.0 != s.as_bytes() {
            Err(serde::de::Error::custom(format!(
                "got {tag:?} instead of `{s}`"
            )))
        } else {
            Ok(())
        }
    }

    // A "streaming" version of `expect_string` that might be optimized for long strings.
    //
    // The default impl doesn't do any streaming, it just reads the string into memory using
    // `expect_string` and then writes it out again.
    #[tracing::instrument(skip(self, write))]
    fn write_string(&mut self, mut write: impl std::io::Write) -> Result<(), Self::Error> {
        write
            .write_all(&self.expect_string()?.0)
            .map_err(|e| serde::de::Error::custom(format!("io error: {e}")))
    }
}

impl<'v, A: SeqAccess<'v>> StringReader<'v> for A {
    type Error = A::Error;

    fn expect_string(&mut self) -> Result<NixString, Self::Error> {
        self.next_element()
            .transpose()
            .unwrap_or_else(|| Err(serde::de::Error::custom("unexpected end")))
    }
}

#[tracing::instrument(skip(seq, sink))]
fn read_entry<'v, 's, A: StringReader<'v>, S: EntrySink<'s> + 's>(
    seq: &mut A,
    sink: S,
) -> Result<(), A::Error> {
    seq.expect_tag("(")?;
    seq.expect_tag("type")?;
    let ty = seq.expect_string()?;
    match ty.0.as_slice() {
        b"regular" => {
            let mut file = sink.become_file();
            // This probably doesn't happen, but the nix source allows multiple settings of "executable"
            let mut tag = seq.expect_string()?;
            while tag.0 == b"executable" {
                // Nix expects an empty string
                seq.expect_tag("")?;
                file.set_executable(true);
                tag = seq.expect_string()?
            }

            if tag.0 == "contents" {
                seq.write_string(file)?;
                seq.expect_tag(")")?;
                Ok(())
            } else if tag.0 == ")" {
                Ok(())
            } else {
                Err(serde::de::Error::custom(format!(
                    "expected contents, got {tag:?}"
                )))
            }
        }
        b"symlink" => {
            seq.expect_tag("target")?;
            let target = seq.expect_string()?;
            seq.expect_tag(")")?;
            sink.become_symlink(target);
            Ok(())
        }
        b"directory" => {
            let mut dir = sink.become_directory();
            loop {
                let tag = seq.expect_string()?;
                if tag.0 == ")" {
                    break Ok(());
                } else if tag.0 == "entry" {
                    seq.expect_tag("(")?;
                    seq.expect_tag("name")?;
                    let name = seq.expect_string()?;
                    let entry = dir.create_entry(name);
                    seq.expect_tag("node")?;
                    read_entry(seq, entry)?;
                    seq.expect_tag(")")?;
                } else {
                    break Err(serde::de::Error::custom(format!(
                        "expected entry, got {tag:?}"
                    )));
                }
            }
        }
        v => Err(serde::de::Error::custom(format!(
            "unknown file type `{v:?}`"
        ))),
    }
}

impl<'v> StringReader<'v> for NixDeserializer<'v> {
    type Error = crate::serialize::Error;

    fn expect_string(&mut self) -> Result<NixString, Self::Error> {
        NixString::deserialize(self)
    }

    #[tracing::instrument(skip(self, write))]
    fn write_string(
        &mut self,
        mut write: impl std::io::Write,
    ) -> Result<(), crate::serialize::Error> {
        let len = self.read_u64()? as usize;
        let mut buf = [0; 4096];
        let mut remaining = len;
        while remaining > 0 {
            let max_len = buf.len().min(remaining);
            let written = self.read.read(&mut buf[0..max_len])?;
            write.write_all(&buf[0..written])?;

            remaining -= written;
        }

        if len % 8 > 0 {
            let padding = 8 - len % 8;
            self.read.read_exact(&mut buf[..padding])?;
        }
        Ok(())
    }
}

/// Stream a Nar from a reader to a writer.
///
// The tricky part is that a Nar isn't framed; in order to know when it ends,
// we actually have to parse the thing. But we don't want to parse and then
// re-serialize it, because we don't want to hold the whole thing in memory. So
// what we do is to parse it into a dummy `EntrySink` (just so we know when the
// Nar ends) while using a `Tee` to simultaneously write the consumed input into
// the output.
#[tracing::instrument(skip(read, write))]
pub fn stream<R: std::io::Read, W: std::io::Write>(
    read: R,
    write: W,
) -> Result<(), crate::serialize::Error> {
    let mut tee = Tee::new(read, write);
    let mut de = NixDeserializer { read: &mut tee };
    de.expect_tag("nix-archive-1")?;
    read_entry(&mut de, &mut Null)?;
    Ok(())
}

impl<'de> Deserialize<'de> for Nar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'v> serde::de::Visitor<'v> for Visitor {
            type Value = Nar;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Nar")
            }

            fn visit_seq<A: SeqAccess<'v>>(self, mut seq: A) -> Result<Nar, A::Error> {
                seq.expect_tag("nix-archive-1")?;
                let mut entry = Nar::default();
                read_entry(&mut seq, &mut entry)?;
                Ok(entry)
            }
        }

        deserializer.deserialize_tuple(usize::MAX, Visitor)
    }
}

impl Serialize for Nar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tup = serializer.serialize_tuple(usize::MAX)?;
        tup.serialize_buf(b"nix-archive-1")?;
        tup.serialize_element(&Untagged(self))?;
        tup.end()
    }
}

struct Untagged<T>(T);

impl<'a> Serialize for Untagged<&'a Nar> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tup = serializer.serialize_tuple(usize::MAX)?;
        tup.serialize_buf(b"(")?;
        tup.serialize_buf(b"type")?;
        match self.0 {
            Nar::Contents(NarFile {
                contents,
                executable,
            }) => {
                tup.serialize_buf(b"regular")?;
                if *executable {
                    tup.serialize_buf(b"executable")?;
                    tup.serialize_buf(b"")?;
                }
                tup.serialize_buf(b"contents")?;
                tup.serialize_element(&contents)?;
            }
            Nar::Target(s) => {
                tup.serialize_buf(b"symlink")?;
                tup.serialize_buf(b"target")?;
                tup.serialize_element(s)?;
            }
            Nar::Directory(entries) => {
                tup.serialize_buf(b"directory")?;
                for entry in entries {
                    tup.serialize_buf(b"entry")?;
                    tup.serialize_buf(b"(")?;
                    tup.serialize_buf(b"name")?;
                    tup.serialize_element(&entry.name)?;
                    tup.serialize_buf(b"node")?;
                    tup.serialize_element(&Untagged(&entry.node))?;
                    tup.serialize_buf(b")")?;
                }
            }
        }
        tup.serialize_buf(b")")?;
        tup.end()
    }
}
