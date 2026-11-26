//! The keys a params value can write (#3449): a serializer that records
//! each top-level struct field the value writes or skips by its
//! `skip_serializing_if`. A field that the type reads but never writes
//! (`#[serde(skip_serializing)]`, a legacy key migrated on load) is not
//! among them, so the listing does not offer it.
//!
//! Provenance: our own code over the public serde data model
//! (https://serde.rs/data-model.html); no other source.

use serde::de::value::Error;
use serde::ser::{self, Impossible, Serialize};

/// The written-or-skipped keys of `value`'s top struct (through one
/// newtype variant). `None` when the value is not a plain struct there
/// (a map, as `#[serde(flatten)]` writes).
pub(crate) fn keys<T: Serialize>(value: &T) -> Option<Vec<&'static str>> {
    let mut keys = Vec::new();
    value.serialize(Recorder { keys: &mut keys, depth: 0 }).ok()?;
    Some(keys)
}

struct Recorder<'k> {
    keys: &'k mut Vec<&'static str>,
    depth: usize,
}

fn refuse<T>() -> Result<T, Error> {
    Err(ser::Error::custom("not a struct"))
}

macro_rules! refuse_all {
    ($($method:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
        fn $method(self, $(_: $ty),*) -> Result<$out, Error> {
            refuse()
        }
    )*};
}

impl<'k> ser::Serializer for Recorder<'k> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Impossible<(), Error>;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Impossible<(), Error>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    refuse_all! {
        serialize_bool(v: bool) -> ();
        serialize_i8(v: i8) -> ();
        serialize_i16(v: i16) -> ();
        serialize_i32(v: i32) -> ();
        serialize_i64(v: i64) -> ();
        serialize_u8(v: u8) -> ();
        serialize_u16(v: u16) -> ();
        serialize_u32(v: u32) -> ();
        serialize_u64(v: u64) -> ();
        serialize_f32(v: f32) -> ();
        serialize_f64(v: f64) -> ();
        serialize_char(v: char) -> ();
        serialize_str(v: &str) -> ();
        serialize_bytes(v: &[u8]) -> ();
        serialize_none() -> ();
        serialize_unit() -> ();
        serialize_unit_struct(n: &'static str) -> ();
        serialize_unit_variant(n: &'static str, i: u32, v: &'static str) -> ();
        serialize_seq(l: Option<usize>) -> Self::SerializeSeq;
        serialize_tuple(l: usize) -> Self::SerializeTuple;
        serialize_tuple_struct(n: &'static str, l: usize) -> Self::SerializeTupleStruct;
        serialize_tuple_variant(n: &'static str, i: u32, v: &'static str, l: usize) -> Self::SerializeTupleVariant;
        serialize_map(l: Option<usize>) -> Self::SerializeMap;
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<(), Error> {
        refuse()
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        if self.depth > 0 {
            return refuse();
        }
        value.serialize(Recorder { keys: self.keys, depth: 1 })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Error> {
        Ok(self)
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self, Error> {
        if self.depth > 0 {
            return refuse();
        }
        Ok(self)
    }
}

impl ser::SerializeStruct for Recorder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, _: &T) -> Result<(), Error> {
        self.keys.push(key);
        Ok(())
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), Error> {
        self.keys.push(key);
        Ok(())
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for Recorder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, _: &T) -> Result<(), Error> {
        self.keys.push(key);
        Ok(())
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), Error> {
        self.keys.push(key);
        Ok(())
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}
