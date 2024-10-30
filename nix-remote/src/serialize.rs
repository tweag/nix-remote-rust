//! Serialization and deserialization for the nix remote protocol.
//!
//! The protocol has two primitive types: integers and byte buffers.
//! All integers are encoded in 64 bits (little endian). I haven't seen signed integers
//! appear in the protocol yet, but presumably they're encoded in twos complement.
//! Byte buffers are encoded as a length (64-bit integer), followed by the bytes in the buffer.
//! If the length of the buffer is not a multiple of 8, it is zero-padded to a multiple of
//! 8 bytes.
//!
//! The Nix source parses the protocol imperatively, but there are some common patterns that
//! we implement declaratively with the help of serde's derive macros:
//! - structs and tuples are serialized as the concatenation of their fields
//!   (Nix does this manually for each struct)
//! - sequences (like `Vec`s) are serialized as a length followed by the concatenation of the
//!   elements (Nix has functions like `readStrings` for this).
//!
//! So for example, the struct
//! ```ignore
//! pub struct BuildPathsWithResults {
//!     paths: Vec<ByteBuf>,
//!     build_mode: u64,
//! }
//! ```
//! gets serde-derived serialization implementations that encode it as:
//! - the number of paths (an int)
//! - the paths concatenated together, each of which consists of
//!    + a length (an int)
//!    + a byte buffer of that length
//! - the build mode (an int)
//!
//! Nix also has some sort of implicit "tagged union", consisting of a type tag (and integer)
//! followed by a body. This serializer does not have built-in support for that, because serde
//! enums are built on string-valued tags (whereas the nix protocol wants integer tags).
//! Instead, we have a separate `tagged_serde` macro for transforming enums into tuples.

use std::io::{Read, Write};

use serde::{de, ser, Serialize};

pub struct Tee<R, W> {
    read: R,
    write: W,
}

impl<R: Read, W: Write> Tee<R, W> {
    pub fn new(read: R, write: W) -> Self {
        Tee { read, write }
    }
}

impl<R: Read, W: Write> Read for Tee<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.read.read(buf)?;
        self.write.write_all(&buf[0..n])?;
        Ok(n)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Won't implement {0}")]
    WontImplement(&'static str),
    #[error("io error {0}")]
    Io(#[from] std::io::Error),
    #[error("Custom {0}")]
    Custom(String),
}

impl de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Error::Custom(msg.to_string())
    }
}

impl ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Error::Custom(msg.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait NixReadExt {
    fn read_nix<'de, 'a: 'de, D: serde::Deserialize<'de>>(&'a mut self) -> Result<D>;
}

impl<R: Read> NixReadExt for R {
    fn read_nix<'de, 'a: 'de, D: serde::Deserialize<'de>>(&'a mut self) -> Result<D> {
        D::deserialize(&mut NixDeserializer { read: self })
    }
}

pub trait NixWriteExt {
    fn write_nix<T: Serialize + ?Sized>(&mut self, val: &T) -> Result<()>;
}

impl<W: Write> NixWriteExt for W {
    fn write_nix<T: Serialize + ?Sized>(&mut self, val: &T) -> Result<()> {
        val.serialize(&mut NixSerializer { write: self })?;
        Ok(())
    }
}

/// A deserializer for the nix remote protocol.
// TODO: should decouple the lifetime of the &mut ref from the lifetime of the Read
pub struct NixDeserializer<'de> {
    pub read: &'de mut dyn Read,
}

/// A serializer for the nix remote protocol.
pub struct NixSerializer<'se> {
    pub write: &'se mut dyn Write,
}

struct Seq<'a, 'de: 'a> {
    deserializer: &'a mut NixDeserializer<'de>,
    len: usize,
}

impl<'a, 'de: 'a> de::SeqAccess<'de> for Seq<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.len > 0 {
            self.len -= 1;
            Ok(Some(de::DeserializeSeed::deserialize(
                seed,
                &mut *self.deserializer,
            )?))
        } else {
            Ok(None)
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.len)
    }
}

impl<'de> NixDeserializer<'de> {
    pub fn read_u64(&mut self) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.read.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn read_byte_buf(&mut self) -> Result<Vec<u8>> {
        // possible errors:
        // Unexecpted EOF
        // IO Error
        // out of memory
        let len = self.read_u64()? as usize;

        // TODO(optimization): don't initialize
        let mut buf = vec![0; len];
        self.read.read_exact(&mut buf)?;

        if len % 8 > 0 {
            let padding = 8 - len % 8;
            let mut pad_buf = [0; 8];
            self.read.read_exact(&mut pad_buf[..padding])?;
        }

        Ok(buf)
    }
}

impl<'se> NixSerializer<'se> {
    pub fn write_byte_buf(&mut self, s: &[u8]) -> Result<()> {
        let len = s.len();

        self.write.write_all(&len.to_le_bytes())?;
        self.write.write_all(s)?;

        if len % 8 > 0 {
            let padding = 8 - len % 8;
            let pad_buf = [0; 8];
            self.write.write_all(&pad_buf[..padding])?;
        };

        Ok(())
    }
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut NixDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("any"))
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_bool(self.read_u64()? != 0)
    }

    fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("i8"))
    }

    fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("i16"))
    }

    fn deserialize_i32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("i32"))
    }

    fn deserialize_i64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("i64"))
    }

    fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("u8"))
    }

    fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("u16"))
    }

    fn deserialize_u32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("u32"))
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u64(self.read_u64()?)
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("f32"))
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("f64"))
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("char"))
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("str"))
    }

    fn deserialize_string<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("String"))
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.read_byte_buf()?)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let tag = self.read_u64()?;
        if tag == 1 {
            visitor.visit_some(self)
        } else {
            visitor.visit_none()
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let len = self.read_u64()? as usize;
        visitor.visit_seq(Seq {
            deserializer: self,
            len,
        })
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_seq(Seq {
            deserializer: self,
            len,
        })
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_seq(Seq {
            deserializer: self,
            len,
        })
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("map"))
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("enum"))
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("ident"))
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::WontImplement("ignored"))
    }
}

impl<'se> ser::SerializeSeq for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> ser::SerializeTuple for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> ser::SerializeTupleStruct for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> ser::SerializeTupleVariant for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> ser::SerializeMap for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        key.serialize(&mut **self)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> ser::SerializeStruct for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> ser::SerializeStructVariant for &mut NixSerializer<'se> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'se> serde::Serializer for &mut NixSerializer<'se> {
    type Ok = ();

    type Error = Error;

    type SerializeSeq = Self;

    type SerializeTuple = Self;

    type SerializeTupleStruct = Self;

    type SerializeTupleVariant = Self;

    type SerializeMap = Self;

    type SerializeStruct = Self;

    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(v as u64)
    }

    fn serialize_i8(self, _v: i8) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("i8"))
    }

    fn serialize_i16(self, _v: i16) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("i16"))
    }

    fn serialize_i32(self, _v: i32) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("i32"))
    }

    fn serialize_i64(self, _v: i64) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("i64"))
    }

    fn serialize_u8(self, _v: u8) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("u8"))
    }

    fn serialize_u16(self, _v: u16) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("u16"))
    }

    fn serialize_u32(self, _v: u32) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("u32"))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(self.write.write_all(&v.to_le_bytes())?)
    }

    fn serialize_f32(self, _v: f32) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("f32"))
    }

    fn serialize_f64(self, _v: f64) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("f64"))
    }

    fn serialize_char(self, _v: char) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("char"))
    }

    fn serialize_str(self, _v: &str) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("String"))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.write_byte_buf(v)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.write.write_all(&0u64.to_le_bytes())?;
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.write.write_all(&1u64.to_le_bytes())?;
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Error::WontImplement("unit variant"))
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.serialize_u64(len.unwrap() as u64)?;
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_tuple(len)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(Error::WontImplement("tuple variant"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(Error::WontImplement("map"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(Error::WontImplement("struct variant"))
    }
}
