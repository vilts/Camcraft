//! The serde shape trace behind `list_operation_types` (#3449): drive a
//! type's own `Deserialize` with a deserializer that answers every call
//! with a sample and records what the type asked for - struct field
//! names, enum variant names, `Option` (optional), sequences and maps.
//! A field that the default value does not serialize is still asked for
//! here, so the listing names every accepted field by one rule.
//!
//! Provenance: our own code over the public serde data model
//! (https://serde.rs/data-model.html); no other source.

use serde::de::value::{Error, StrDeserializer};
use serde::de::{self, DeserializeOwned, DeserializeSeed, IntoDeserializer, Visitor};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// Deeper than any params DTO nests; stops a recursive type.
const MAX_DEPTH: usize = 16;
/// Each pass skips one more field whose sample the type refused.
const MAX_PASSES: usize = 256;

/// What the type asked for at one path (`pocket.rest.previous_tool_diameter`;
/// a sequence element is `<path>[]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Node {
    /// `boolean`, `number`, `string`, `object`, `array` or `any` (the type
    /// reads itself: untagged, internally tagged or hand-written).
    pub ty: &'static str,
    /// An `Option`: absent is accepted.
    pub optional: bool,
    /// The variant names of an enum, in declaration order.
    pub values: Vec<&'static str>,
    /// The field names of a struct, in declaration order.
    pub fields: Vec<&'static str>,
    /// A map whose keys the type does not name (a real map, or a struct
    /// with `#[serde(flatten)]`).
    pub opaque: bool,
}

#[derive(Default)]
struct Ctx {
    nodes: BTreeMap<String, Node>,
    skip: BTreeSet<String>,
    failed: Option<String>,
    /// The variant to follow at the root enum (a kind's file key).
    pick: Option<String>,
}

/// Traces `T` with its root at `root`. At a root enum the variant `pick`
/// is followed; every other enum follows its first variant.
pub(crate) fn trace<T: DeserializeOwned>(root: &str, pick: Option<&str>) -> BTreeMap<String, Node> {
    let ctx = RefCell::new(Ctx { pick: pick.map(str::to_owned), ..Ctx::default() });
    for _ in 0..MAX_PASSES {
        ctx.borrow_mut().failed = None;
        let done = T::deserialize(Tracer { ctx: &ctx, path: root.to_owned(), depth: 0 }).is_ok();
        let mut c = ctx.borrow_mut();
        match c.failed.take() {
            Some(p) if !done && !c.skip.contains(&p) => {
                c.skip.insert(p);
            }
            _ => break,
        }
    }
    ctx.into_inner().nodes
}

fn child(path: &str, name: &str) -> String {
    if path.is_empty() { name.to_owned() } else { format!("{path}.{name}") }
}

struct Tracer<'c> {
    ctx: &'c RefCell<Ctx>,
    path: String,
    depth: usize,
}

impl Tracer<'_> {
    fn mark(&self, f: impl FnOnce(&mut Node)) -> Result<(), Error> {
        let mut c = self.ctx.borrow_mut();
        f(c.nodes.entry(self.path.clone()).or_default());
        if self.depth > MAX_DEPTH {
            return Err(de::Error::custom("trace too deep"));
        }
        Ok(())
    }
    fn ty(&self, ty: &'static str) -> Result<(), Error> {
        self.mark(|n| n.ty = ty)
    }
    fn at(&self, path: String) -> Self {
        Tracer { ctx: self.ctx, path, depth: self.depth.saturating_add(1) }
    }
}

macro_rules! sample {
    ($($method:ident => $ty:literal, $visit:ident($($v:expr)?);)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            self.ty($ty)?;
            visitor.$visit($($v)?)
        }
    )*};
}

impl<'de> de::Deserializer<'de> for Tracer<'_> {
    type Error = Error;

    sample! {
        deserialize_bool => "boolean", visit_bool(false);
        deserialize_i8 => "number", visit_i64(0);
        deserialize_i16 => "number", visit_i64(0);
        deserialize_i32 => "number", visit_i64(0);
        deserialize_i64 => "number", visit_i64(0);
        deserialize_u8 => "number", visit_u64(0);
        deserialize_u16 => "number", visit_u64(0);
        deserialize_u32 => "number", visit_u64(0);
        deserialize_u64 => "number", visit_u64(0);
        deserialize_f32 => "number", visit_f64(0.0);
        deserialize_f64 => "number", visit_f64(0.0);
        deserialize_char => "string", visit_char('a');
        deserialize_str => "string", visit_str("");
        deserialize_string => "string", visit_str("");
        deserialize_bytes => "string", visit_bytes(&[]);
        deserialize_byte_buf => "string", visit_bytes(&[]);
        deserialize_identifier => "string", visit_str("");
        deserialize_unit => "null", visit_unit();
        deserialize_any => "any", visit_unit();
        deserialize_ignored_any => "any", visit_unit();
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error> {
        self.ty("null")?;
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.mark(|n| n.optional = true)?;
        visitor.visit_some(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.ty("array")?;
        let elem = self.at(format!("{}[]", self.path));
        visitor.visit_seq(Seq { tracer: Some(elem), left: 1 })
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        self.ty("array")?;
        let elem = self.at(format!("{}[]", self.path));
        visitor.visit_seq(Seq { tracer: Some(elem), left: len })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(self, _name: &'static str, len: usize, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.mark(|n| {
            n.ty = "object";
            n.opaque = true;
        })?;
        visitor.visit_map(Fields { tracer: self, fields: &[], at: 0 })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.mark(|n| {
            n.ty = "object";
            n.fields = fields.to_vec();
        })?;
        visitor.visit_map(Fields { tracer: self, fields, at: 0 })
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.mark(|n| {
            n.ty = "string";
            n.values = variants.to_vec();
        })?;
        let wanted = if self.depth == 0 { self.ctx.borrow().pick.clone() } else { None };
        let variant = match wanted {
            Some(w) => variants.iter().copied().find(|v| *v == w).ok_or_else(|| de::Error::custom("no such variant"))?,
            None => variants.first().copied().ok_or_else(|| de::Error::custom("an enum without variants"))?,
        };
        visitor.visit_enum(Variant { tracer: self, variant })
    }
}

struct Seq<'c> {
    tracer: Option<Tracer<'c>>,
    left: usize,
}

impl<'de> de::SeqAccess<'de> for Seq<'_> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Error> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        match &self.tracer {
            Some(t) => seed.deserialize(t.at(t.path.clone())).map(Some),
            None => Ok(None),
        }
    }
}

/// A struct's keys, each once, minus the fields an earlier pass skipped.
struct Fields<'c> {
    tracer: Tracer<'c>,
    fields: &'static [&'static str],
    at: usize,
}

impl Fields<'_> {
    fn path(&self) -> Option<String> {
        self.fields.get(self.at).map(|f| child(&self.tracer.path, f))
    }
}

impl<'de> de::MapAccess<'de> for Fields<'_> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        while let Some(p) = self.path() {
            if !self.tracer.ctx.borrow().skip.contains(&p) {
                break;
            }
            self.at += 1;
        }
        match self.fields.get(self.at) {
            Some(name) => {
                let key: StrDeserializer<'_, Error> = name.into_deserializer();
                seed.deserialize(key).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, Error> {
        let path = self.path().ok_or_else(|| de::Error::custom("a value without a key"))?;
        self.at += 1;
        let out = seed.deserialize(self.tracer.at(path.clone()));
        if out.is_err() {
            let mut c = self.tracer.ctx.borrow_mut();
            if c.failed.is_none() {
                c.failed = Some(path);
            }
        }
        out
    }
}

struct Variant<'c> {
    tracer: Tracer<'c>,
    variant: &'static str,
}

impl<'de, 'c> de::EnumAccess<'de> for Variant<'c> {
    type Error = Error;
    type Variant = Tracer<'c>;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Tracer<'c>), Error> {
        let key: StrDeserializer<'_, Error> = self.variant.into_deserializer();
        let content = self.tracer.at(child(&self.tracer.path, self.variant));
        Ok((seed.deserialize(key)?, content))
    }
}

impl<'de> de::VariantAccess<'de> for Tracer<'_> {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        Ok(())
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(self)
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_tuple(self, len, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_struct(self, "", fields, visitor)
    }
}
