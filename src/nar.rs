use std::iter::Peekable;

use serde::{de::SeqAccess, ser::SerializeTuple, Deserialize, Serialize};

use crate::NixString;

#[derive(Clone, Debug, Default)]
pub struct Nar {
    entries: Vec<NarEntry>,
}

#[derive(Clone, Debug)]
pub enum NarEntry {
    Contents {
        contents: NixString,
        executable: bool,
    },
    Target(NixString),
    Directory(Nar),
}

// TODO: if tagged_serde supported tagging with arbitrary ser/de types,
// we could use it here
#[derive(Clone, Debug)]
pub enum NarDirectoryEntry {
    Name(NixString),
    Node(Nar),
}

pub struct PushBackable<I: Iterator> {
    iter: I,
    head: Option<I::Item>,
}

impl<I: Iterator> Iterator for PushBackable<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.head.take().or_else(|| self.iter.next())
    }
}

impl<I: Iterator> PushBackable<I> {
    pub fn new(iter: I) -> Self {
        Self { iter, head: None }
    }

    pub fn push_back(&mut self, item: I::Item) {
        if self.head.is_some() {
            panic!("too much pushing");
        }
        self.head = Some(item);
    }
}

impl Serialize for Nar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut tup = serializer.serialize_tuple(usize::MAX)?;
        tup.serialize_element("(")?;
        for entry in &self.entries {
            tup.serialize_element(entry)?;
        }
        tup.serialize_element(")")?;
        tup.end()
    }
}

fn expect_tag<E, I>(seq: &mut I, s: &str) -> Result<(), E>
where
    I: Iterator<Item = Result<NixString, E>>,
    E: serde::de::Error,
{
    let tag = expect_string(seq)?;
    if tag.0 != s.as_bytes() {
        Err(serde::de::Error::custom(format!(
            "got {tag:?} instead of `{s}`"
        )))
    } else {
        Ok(())
    }
}

fn expect_string<E, I>(seq: &mut I) -> Result<NixString, E>
where
    I: Iterator<Item = Result<NixString, E>>,
    E: serde::de::Error,
{
    seq.next()
        .unwrap_or_else(|| Err(serde::de::Error::custom("unexpected end")))
}

struct SeqIter<'v, A: SeqAccess<'v>>(A, std::marker::PhantomData<&'v mut ()>);

impl<'v, A: SeqAccess<'v>> Iterator for SeqIter<'v, A> {
    type Item = Result<NixString, A::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next_element().transpose()
    }
}

fn read_nar<E, I>(seq: &mut PushBackable<I>) -> Result<Nar, E>
where
    I: Iterator<Item = Result<NixString, E>>,
    E: serde::de::Error,
{
    expect_tag(&mut seq.iter, "(")?;

    let mut ret = Nar::default();
    loop {
        let next = seq
            .next()
            .unwrap_or_else(|| Err(serde::de::Error::custom("unexpected end".to_owned())))?;
        if next.0 == b")" {
            return Ok(ret);
        }
        ret.entries.push(read_entry(seq)?);
    }
}

fn read_entry<E, I>(seq: &mut PushBackable<I>) -> Result<NarEntry, E>
where
    I: Iterator<Item = Result<NixString, E>>,
    E: serde::de::Error,
{
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
                Ok(NarEntry::Contents {
                    contents,
                    executable,
                })
            } else if tag.0 == ")" {
                seq.push_back(Ok(tag));
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
            Ok(NarEntry::Target(target))
        }
        b"directory" => Ok(NarEntry::Directory(read_nar(seq)?)),
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
                // TODO: factor out into a parse function
                let iter = SeqIter(&mut seq, std::marker::PhantomData);
                let mut pushable = PushBackable::new(iter);
                read_nar(&mut pushable)
            }
        }

        todo!()
    }
}

impl Serialize for NarEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            NarEntry::Contents {
                contents,
                executable,
            } => {
                if *executable {
                    ("executable", "", "contents", &contents).serialize(serializer)
                } else {
                    ("contents", &contents).serialize(serializer)
                }
            }
            NarEntry::Target(s) => ("target", s).serialize(serializer),
            NarEntry::Directory(entries) => {
                // FIXME: copy-paste from Nar
                let mut tup = serializer.serialize_tuple(usize::MAX)?;
                tup.serialize_element("(")?;
                for entry in &entries.entries {
                    tup.serialize_element(entry)?;
                }
                tup.serialize_element(")")?;
                tup.end()
            }
        }
    }
}
