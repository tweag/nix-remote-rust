use serde::{de::SeqAccess, ser::SerializeTuple, Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::{
    serialize::{NixDeserializer, NixSerializer},
    NixString,
};

#[derive(Clone, Debug)]
pub struct Nar(NarEntry);

#[derive(Clone, Debug, Default)]
pub struct NarFile {
    pub contents: NixString,
    pub executable: bool,
}

#[derive(Clone, Debug)]
pub enum NarEntry {
    Contents(NarFile),
    Target(NixString),
    Directory(Vec<NarDirectoryEntry>),
}

// TODO: if tagged_serde supported tagging with arbitrary ser/de types,
// we could use it here
#[derive(Clone, Debug)]
pub struct NarDirectoryEntry {
    name: NixString,
    node: NarEntry,
}

trait EntryCallback<'a, S: EntrySink<'a>> {
    fn call(self, val: S);
}

trait EntrySink<'a>: 'a {
    type DirectorySink: DirectorySink<'a>;
    type FileSink: FileSink;
    // this could get threaded through to write out the )s
    // type Context;

    fn become_directory(self) -> Self::DirectorySink;
    fn become_file(self) -> Self::FileSink;
    fn become_symlink(self, target: NixString);
}

trait DirectorySinkSuper {
    type EntrySink<'b>: EntrySink<'b>;
}

trait DirectorySink<'a>: DirectorySinkSuper {
    // The extra bounds are because of
    // https://github.com/rust-lang/rust/issues/87479
    // type EntrySink<'b>: EntrySink<'b>
    // where
    //     Self: 'a,
    //     'a: 'b,
    //     Self: 'b;

    // fn create_entry<'b>(&'b mut self, ctx: Self::Context, name: NixString) -> Self::EntrySink<'b>
    // where
    //     'a: 'b;

    fn with_entry<'b, C>(&'b mut self, name: NixString, callback: C)
    where
        Self: 'a,
        'a: 'b,
        C: for<'c> EntryCallback<'c, Self::EntrySink<'c>>;

    //
}

trait FileSink {
    fn set_executable(&mut self, executable: bool);
    fn add_contents(&mut self, contents: &[u8]);
}

impl<'a> EntrySink<'a> for NixSerializer<'a> {
    type DirectorySink = Self;

    type FileSink = Self;

    fn become_directory(mut self) -> Self::DirectorySink {
        (&mut self).serialize_buf(b"directory").unwrap();
        self
    }

    fn become_file(self) -> Self::FileSink {
        (&mut self).serialize_buf(b"regular").unwrap();
        if *executable {
            (&mut self).serialize_buf(b"executable").unwrap();
            (&mut self).serialize_buf(b"").unwrap();
        }
        (&mut self).serialize_buf(b"contents").unwrap();
    }

    fn become_symlink(self, target: NixString) {
        todo!()
    }
}

impl<'a> DirectorySinkSuper for NixSerializer<'a> {
    type EntrySink<'b> = NixSerializer<'b>;
}

impl<'a> DirectorySink<'a> for NixSerializer<'a> {
    // type EntrySink<'b> = NixSerializer<'b>
    // where
    //     Self: 'a,
    //     'a: 'b,
    //     Self: 'b;

    // fn create_entry<'b>(&'b mut self, name: NixString) -> Self::EntrySink<'b>
    // where
    //     'a: 'b,
    // {
    //     // Note that we need to write out a ) after the entry is done. The trait might need a commit function.
    //     todo!()
    // }

    fn with_entry<'b, C>(&'b mut self, name: NixString, callback: C)
    where
        Self: 'a,
        'a: 'b,
        C: for<'c> EntryCallback<'c, Self::EntrySink<'c>>,
    {
        (&mut *self).serialize_buf(b"entry").unwrap();
        (&mut *self).serialize_buf(b"(").unwrap();
        (&mut *self).serialize_buf(b"name").unwrap();
        (&mut *self).serialize_element(&name).unwrap();
        (&mut *self).serialize_buf(b"node").unwrap();
        {
            let x = NixSerializer {
                write: &mut *self.write,
            };
            callback.call(x);
        }
        (&mut *self).serialize_buf(b")").unwrap();
        todo!()
    }
}

impl<'a> FileSink for NixSerializer<'a> {
    fn set_executable(&mut self, executable: bool) {
        todo!()
    }

    fn add_contents(&mut self, contents: &[u8]) {
        todo!()
    }
}

impl<'a> EntrySink<'a> for &'a mut NarEntry {
    type DirectorySink = &'a mut Vec<NarDirectoryEntry>;
    type FileSink = &'a mut NarFile;

    fn become_directory(self) -> Self::DirectorySink {
        *self = NarEntry::Directory(Vec::new());
        let NarEntry::Directory(dir) = self else { unreachable!() };
        dir
    }

    fn become_file(self) -> Self::FileSink {
        *self = NarEntry::Contents(NarFile {
            executable: false,
            contents: NixString::default(),
        });
        // TODO: can we express this better?
        let NarEntry::Contents(contents) = self else {
            unreachable!()
        };
        contents
    }

    fn become_symlink(self, target: NixString) {
        *self = NarEntry::Target(target);
    }
}

impl<'a> DirectorySinkSuper for &'a mut Vec<NarDirectoryEntry> {
    type EntrySink<'b> = &'b mut NarEntry;
}

impl<'a> DirectorySink<'a> for &'a mut Vec<NarDirectoryEntry> {
    fn with_entry<'b, C>(&'b mut self, name: NixString, callback: C)
    where
        Self: 'a,
        'a: 'b,
        C: for<'c> EntryCallback<'c, Self::EntrySink<'c>>,
    {
        self.push(NarDirectoryEntry {
            name,
            node: NarEntry::Contents(NarFile {
                contents: NixString::default(),
                executable: false,
            }),
        });
        callback.call(&mut self.last_mut().unwrap().node)
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

trait SerializeTupleExt: SerializeTuple {
    fn serialize_buf(&mut self, s: impl AsRef<[u8]>) -> Result<(), Self::Error> {
        self.serialize_element(&ByteBuf::from(s.as_ref()))
    }
}

impl<S: SerializeTuple> SerializeTupleExt for S {}

impl Serialize for Nar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tup = serializer.serialize_tuple(usize::MAX)?;
        tup.serialize_buf(b"nix-archive-1")?;
        tup.serialize_element(&self.0)?;
        tup.end()
    }
}

fn expect_tag<'v, A: SeqAccess<'v>>(seq: &mut A, s: &str) -> Result<(), A::Error> {
    let tag = expect_string(seq)?;
    if tag.0 != s.as_bytes() {
        Err(serde::de::Error::custom(format!(
            "got {tag:?} instead of `{s}`"
        )))
    } else {
        Ok(())
    }
}

fn expect_string<'v, A: SeqAccess<'v>>(seq: &mut A) -> Result<NixString, A::Error> {
    seq.next_element()
        .transpose()
        .unwrap_or_else(|| Err(serde::de::Error::custom("unexpected end")))
}

struct ECallback<'w, 'v, A: SeqAccess<'v>>(&'w mut A, core::marker::PhantomData<&'v mut ()>);

impl<'w, 'v, 'c, A: SeqAccess<'v>, S: EntrySink<'c> + 'c> EntryCallback<'c, S>
    for ECallback<'w, 'v, A>
{
    fn call(self, entry: S) {
        expect_tag(self.0, "node").unwrap();
        read_entry(self.0, entry).unwrap();
        expect_tag(self.0, ")").unwrap();
    }
}

fn read_entry<'v, 's, A: SeqAccess<'v>, S: EntrySink<'s> + 's>(
    seq: &mut A,
    sink: S,
) -> Result<(), A::Error> {
    expect_tag(seq, "(")?;
    expect_tag(seq, "type")?;
    let ty = expect_string(seq)?;
    match ty.0.as_slice() {
        b"regular" => {
            let mut file = sink.become_file();
            // This probably doesn't happen, but the nix source allows multiple settings of "executable"
            let mut tag = expect_string(seq)?;
            while tag.0 == b"executable" {
                // Nix expects an empty string
                expect_tag(seq, "")?;
                file.set_executable(true);
                tag = expect_string(seq)?
            }

            if tag.0 == "contents" {
                // TODO: can also read the contents in chunks
                let contents = expect_string(seq)?;
                expect_tag(seq, ")")?;
                file.add_contents(&contents.0);
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
            expect_tag(seq, "target")?;
            let target = expect_string(seq)?;
            expect_tag(seq, ")")?;
            sink.become_symlink(target);
            Ok(())
        }
        b"directory" => {
            let mut dir = sink.become_directory();
            loop {
                let tag = expect_string(seq)?;
                if tag.0 == ")" {
                    break Ok(());
                } else if tag.0 == "entry" {
                    expect_tag(seq, "(")?;
                    expect_tag(seq, "name")?;
                    let name = expect_string(seq)?;
                    dir.with_entry(name, ECallback(seq, core::marker::PhantomData));
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
                expect_tag(&mut seq, "nix-archive-1")?;
                let mut entry = NarEntry::Contents(NarFile::default());
                read_entry(&mut seq, &mut entry)?;
                Ok(Nar(entry))
            }
        }

        deserializer.deserialize_tuple(usize::MAX, Visitor)
    }
}

impl Serialize for NarEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tup = serializer.serialize_tuple(usize::MAX)?;
        tup.serialize_buf(b"(")?;
        tup.serialize_buf(b"type")?;
        match self {
            NarEntry::Contents(NarFile {
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
            NarEntry::Target(s) => {
                tup.serialize_buf(b"symlink")?;
                tup.serialize_buf(b"target")?;
                tup.serialize_element(s)?;
            }
            NarEntry::Directory(entries) => {
                tup.serialize_buf(b"directory")?;
                for entry in entries {
                    tup.serialize_buf(b"entry")?;
                    tup.serialize_buf(b"(")?;
                    tup.serialize_buf(b"name")?;
                    tup.serialize_element(&entry.name)?;
                    tup.serialize_buf(b"node")?;
                    tup.serialize_element(&entry.node)?;
                    tup.serialize_buf(b")")?;
                }
            }
        }
        tup.serialize_buf(b")")?;
        tup.end()
    }
}
