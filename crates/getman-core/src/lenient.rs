//! Deserializers that accept any scalar where a string is expected.
//!
//! People write `port: 3000` or `debug: true` in YAML, which parse as a
//! number and a boolean. Variables and values are text, so these read every
//! scalar as the text it was written as, and a missing value as empty.

use std::fmt;

use serde::Deserializer;
use serde::de::{self, MapAccess, Visitor};

use crate::model::Vars;

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
}

/// Reads any scalar as a string.
pub(crate) fn string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    deserializer.deserialize_any(ScalarVisitor)
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
}

/// Reads a map whose keys and values may be any scalar.
pub(crate) fn vars<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vars, D::Error> {
    deserializer.deserialize_any(VarsVisitor)
}
