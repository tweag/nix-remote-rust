use serde::{de::SeqAccess, ser::SerializeTuple, Deserialize, Serialize};

use crate::NixString;

#[derive(Clone, Debug)]
pub struct Nar(NarEntry);

#[derive(Clone, Debug)]
pub enum NarEntry {
    Contents {
        contents: NixString,
        executable: bool,
    },
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

impl Serialize for Nar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tup = serializer.serialize_tuple(usize::MAX)?;
        tup.serialize_element("nix-archive-1")?;
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

fn read_entry<'v, A: SeqAccess<'v>>(seq: &mut A) -> Result<NarEntry, A::Error> {
    expect_tag(seq, "(")?;
    expect_tag(seq, "type")?;
    let ty = expect_string(seq)?;
    match ty.0.as_slice() {
        b"regular" => {
            let mut executable = false;
            // This probably doesn't happen, but the nix source allows multiple settings of "executable"
            let mut tag = expect_string(seq)?;
            while tag.0 == b"executable" {
                // Nix expects an empty string
                expect_tag(seq, "")?;
                executable = true;
                tag = expect_string(seq)?
            }

            if tag.0 == "contents" {
                let contents = expect_string(seq)?;
                expect_tag(seq, ")")?;
                Ok(NarEntry::Contents {
                    contents,
                    executable,
                })
            } else if tag.0 == ")" {
                Ok(NarEntry::Contents {
                    contents: Default::default(),
                    executable,
                })
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
            Ok(NarEntry::Target(target))
        }
        b"directory" => {
            let mut entries = Vec::new();
            loop {
                let tag = expect_string(seq)?;
                if tag.0 == ")" {
                    break Ok(NarEntry::Directory(entries));
                } else if tag.0 == "entry" {
                    expect_tag(seq, "(")?;
                    expect_tag(seq, "name")?;
                    let name = expect_string(seq)?;
                    expect_tag(seq, "node")?;
                    let node = read_entry(seq)?;
                    expect_tag(seq, ")")?;
                    entries.push(NarDirectoryEntry { name, node });
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
                read_entry(&mut seq).map(Nar)
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
        tup.serialize_element("(")?;
        tup.serialize_element("type")?;
        match self {
            NarEntry::Contents {
                contents,
                executable,
            } => {
                tup.serialize_element("regular")?;
                if *executable {
                    tup.serialize_element("executable")?;
                    tup.serialize_element("")?;
                }
                tup.serialize_element("contents")?;
                tup.serialize_element(&contents)?;
            }
            NarEntry::Target(s) => {
                tup.serialize_element("symlink")?;
                tup.serialize_element("target")?;
                tup.serialize_element(s)?;
            }
            NarEntry::Directory(entries) => {
                tup.serialize_element("directory")?;
                // FIXME: copy-paste from Nar
                for entry in entries {
                    tup.serialize_element("entry")?;
                    tup.serialize_element("(")?;
                    tup.serialize_element("name")?;
                    tup.serialize_element(&entry.name)?;
                    tup.serialize_element("node")?;
                    tup.serialize_element(&entry.node)?;
                    tup.serialize_element(")")?;
                }
            }
        }
        tup.serialize_element(")")?;
        tup.end()
    }
}
