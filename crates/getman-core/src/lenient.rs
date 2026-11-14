//! Deserializers that accept any scalar where a string is expected.
//!
//! People write `port: 3000` or `debug: true` in YAML, which parse as a
//! number and a boolean. Variables and values are text, so these read every
//! scalar as the text it was written as, and a missing value as empty.
//!
//! They ask for the text with a newtype named [`TEXT`]. The YAML reader in
//! [`crate::yaml`] answers with the scalar as written, so `1.0` stays `1.0`
//! and `0x10` stays `0x10`. Other formats, such as JSON over the local API,
//! pass the value through, and a number or a boolean becomes its text.
//!
//! Maps tagged with `type`, such as `auth` and `body`, are read with
//! [`tagged`]. Serde holds every value of such a map before it knows which
//! variant it reads, so [`tagged`] asks for all of them as text, and the
//! fields that are not text read theirs with [`boolean`] and
//! [`whole_number`].

use std::fmt;
use std::marker::PhantomData;

use serde::Deserializer;
use serde::de::{self, MapAccess, Unexpected, Visitor};

use crate::model::Vars;

/// The newtype name a field reads through to ask for text.
pub(crate) const TEXT: &str = "$getman::lenient::Text";

struct ScalarVisitor;

impl<'de> Visitor<'de> for ScalarVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string, number, boolean or nothing")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
        Ok(value.to_owned())
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<String, E> {
        Ok(value)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<String, E> {
        Ok(value.to_string())
    }

    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        Ok(String::new())
    }

    fn visit_none<E: de::Error>(self) -> Result<String, E> {
        Ok(String::new())
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<String, D::Error> {
        deserializer.deserialize_any(self)
    }
}

/// Reads any scalar as a string.
pub(crate) fn string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    deserializer.deserialize_newtype_struct(TEXT, ScalarVisitor)
}

struct Scalar(String);

impl<'de> de::Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        string(deserializer).map(Scalar)
    }
}

struct VarsVisitor;

impl<'de> Visitor<'de> for VarsVisitor {
    type Value = Vars;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map of names to values")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vars, A::Error> {
        let mut vars = Vars::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((Scalar(name), Scalar(value))) = map.next_entry()? {
            vars.insert(name, value);
        }
        Ok(vars)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Vars, E> {
        Ok(Vars::new())
    }

    fn visit_none<E: de::Error>(self) -> Result<Vars, E> {
        Ok(Vars::new())
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<Vars, D::Error> {
        deserializer.deserialize_any(self)
    }
}

/// Reads a map whose keys and values may be any scalar.
pub(crate) fn vars<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vars, D::Error> {
    deserializer.deserialize_newtype_struct(TEXT, VarsVisitor)
}

/// A map tagged with `type`, read by its derived code.
pub(crate) trait Tagged<'de>: Sized {
    /// Reads the map as serde's derived code does.
    fn derived<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}

struct TaggedVisitor<T>(PhantomData<T>);

impl<'de, T: Tagged<'de>> Visitor<'de> for TaggedVisitor<T> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map with a type")
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        T::derived(deserializer)
    }
}

/// Reads a map tagged with `type`, asking for every value in it as text.
pub(crate) fn tagged<'de, D: Deserializer<'de>, T: Tagged<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    deserializer.deserialize_newtype_struct(TEXT, TaggedVisitor(PhantomData))
}

struct BooleanVisitor;

impl Visitor<'_> for BooleanVisitor {
    type Value = bool;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a boolean")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<bool, E> {
        Ok(value)
    }

    // YAML's own spellings of a boolean.
    fn visit_str<E: de::Error>(self, value: &str) -> Result<bool, E> {
        match value {
            "true" | "True" | "TRUE" => Ok(true),
            "false" | "False" | "FALSE" => Ok(false),
            _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
        }
    }
}

/// Reads a boolean, or the text of one, as a map tagged with `type` holds
/// it.
pub(crate) fn boolean<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    deserializer.deserialize_any(BooleanVisitor)
}

struct WholeNumberVisitor;

impl<'de> Visitor<'de> for WholeNumberVisitor {
    type Value = Option<u64>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a whole number")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Option<u64>, E> {
        Ok(Some(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Option<u64>, E> {
        u64::try_from(value)
            .map(Some)
            .map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
    }

    // The text of a number, read by YAML's own rules, so `0x10` is 16.
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Option<u64>, E> {
        serde_yaml_ng::from_str(value)
            .map(Some)
            .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Option<u64>, E> {
        Ok(None)
    }

    fn visit_none<E: de::Error>(self) -> Result<Option<u64>, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Option<u64>, D::Error> {
        deserializer.deserialize_any(self)
    }
}

/// Reads a whole number that may be missing, or the text of one, as a map
/// tagged with `type` holds it.
pub(crate) fn whole_number<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    deserializer.deserialize_option(WholeNumberVisitor)
}
