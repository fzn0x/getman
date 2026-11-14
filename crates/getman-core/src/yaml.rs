//! Reading YAML so that a scalar keeps its text where the model asks for
//! text.
//!
//! serde_yaml_ng resolves a plain scalar such as `1.0` or `0x10` to a number
//! when the type being read accepts any value (`deserialize_any`). Only when
//! the type asks for a string does it hand over the scalar's text, as
//! written. Getman's text fields accept any value, so that `port: 3000`
//! reads, and serde reads every value of a map tagged with `type`, such as
//! `auth`, that way too. Both got the number back: `1.0` became `1`. The
//! library's own `Value` keeps the parsed number as well, not the text.
//!
//! Asking for a string is the only way to the text, but it fails on a map
//! or a list, and the library cannot say which comes next without reading
//! it. So [`from_str`] reads a document twice. The first pass learns its
//! shape and what each scalar resolves to. The second asks for every scalar
//! as a string. The value is then read from that tree: a field that asks
//! for a number, a boolean or null gets the resolved value, as it always
//! has, and a field that asks for text gets the text.

use std::fmt;

use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, Deserializer, IgnoredAny, IntoDeserializer, MapAccess,
    SeqAccess, Unexpected, Visitor,
};
use serde_yaml_ng::{Error, Value};

use crate::lenient;

/// Reads a YAML document as `T`, keeping each scalar as written where `T`
/// asks for text.
pub fn from_str<T: DeserializeOwned>(text: &str) -> Result<T, Error> {
    read(text, &parse(text)?)
}

/// Like [`from_str`], for a document that is a mapping with one of `keys`
/// at its top. Other documents give `None`.
pub(crate) fn from_str_with_key<T: DeserializeOwned>(
    text: &str,
    keys: &[&str],
) -> Result<Option<T>, Error> {
    let shape: Node = serde_yaml_ng::from_str(text)?;
    let Node::Map(entries) = &shape else {
        return Ok(None);
    };
    let has_key = entries.iter().any(|(key, _)| match key {
        Node::Scalar { value, .. } => value.as_str().is_some_and(|key| keys.contains(&key)),
        _ => false,
    });
    if !has_key {
        return Ok(None);
    }
    read(text, &with_text(text, shape)?).map(Some)
}

fn parse(text: &str) -> Result<Node, Error> {
    with_text(text, serde_yaml_ng::from_str(text)?)
}

/// The second pass: fills in the text of every scalar of `shape`.
fn with_text(text: &str, mut shape: Node) -> Result<Node, Error> {
    // An empty document has no scalar to read the text of.
    if !shape.is_null() {
        Fill(&mut shape).deserialize(serde_yaml_ng::Deserializer::from_str(text))?;
    }
    Ok(shape)
}

fn read<T: DeserializeOwned>(text: &str, node: &Node) -> Result<T, Error> {
    T::deserialize(Reader::new(node)).map_err(|error| {
        // The tree does not know where a value was in the file. When the
        // library reading the file directly fails the same way, its error
        // says where.
        match serde_yaml_ng::from_str::<T>(text) {
            Err(located) if located.to_string().contains(&error.to_string()) => located,
            _ => error,
        }
    })
}

/// A YAML value, with each scalar's text and what it resolves to.
#[derive(Debug)]
enum Node {
    /// A scalar: its text as written, and its value: null, a boolean, a
    /// number or a string.
    Scalar {
        text: String,
        value: Value,
    },
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

impl Node {
    fn scalar(value: Value) -> Self {
        Self::Scalar {
            text: String::new(),
            value,
        }
    }

    fn is_null(&self) -> bool {
        matches!(
            self,
            Self::Scalar {
                value: Value::Null,
                ..
            }
        )
    }

    fn unexpected(&self) -> Unexpected<'_> {
        match self {
            Self::Scalar { text, .. } => Unexpected::Str(text),
            Self::Seq(_) => Unexpected::Seq,
            Self::Map(_) => Unexpected::Map,
        }
    }
}

/// The first pass: reads the shape of a document and how its scalars
/// resolve.
impl<'de> de::Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ShapeVisitor)
    }
}

struct ShapeVisitor;

impl<'de> Visitor<'de> for ShapeVisitor {
    type Value = Node;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any YAML value")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Node, E> {
        Ok(Node::scalar(Value::Bool(value)))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Node, E> {
        Ok(Node::scalar(Value::Number(value.into())))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Node, E> {
        Ok(Node::scalar(Value::Number(value.into())))
    }

    // Integers too large for a YAML number are read as text.
    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Node, E> {
        Ok(Node::scalar(Value::String(value.to_string())))
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Node, E> {
        Ok(Node::scalar(Value::String(value.to_string())))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Node, E> {
        Ok(Node::scalar(Value::Number(value.into())))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Node, E> {
        Ok(Node::scalar(Value::String(value.to_owned())))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::scalar(Value::Null))
    }

    fn visit_none<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::scalar(Value::Null))
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Node, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Node::Seq(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(entry) = map.next_entry()? {
            entries.push(entry);
        }
        Ok(Node::Map(entries))
    }

    // A value with a tag of its own, such as `!env HOME`, reads as the
    // value itself, as it does where a string is asked for.
    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<Node, A::Error> {
        let (IgnoredAny, value) = data.variant()?;
        de::VariantAccess::newtype_variant(value)
    }
}

/// The second pass: reads every scalar of a node as a string, which
/// serde_yaml_ng answers with the scalar's text as written.
struct Fill<'a>(&'a mut Node);

/// The two passes read the same text, so they see the same document. If
/// they ever did not, this is the error.
const CHANGED: &str = "the YAML document read differently the second time";

impl<'de> DeserializeSeed<'de> for Fill<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        match self.0 {
            Node::Scalar { text, .. } => {
                *text = deserializer.deserialize_str(TextVisitor)?;
                Ok(())
            }
            Node::Seq(items) => deserializer.deserialize_seq(FillSeq(items)),
            Node::Map(entries) => deserializer.deserialize_map(FillMap(entries)),
        }
    }
}

struct TextVisitor;

impl Visitor<'_> for TextVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a scalar")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
        Ok(value.to_owned())
    }
}

struct FillSeq<'a>(&'a mut [Node]);

impl<'de> Visitor<'de> for FillSeq<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        for item in self.0 {
            seq.next_element_seed(Fill(item))?
                .ok_or_else(|| de::Error::custom(CHANGED))?;
        }
        match seq.next_element::<IgnoredAny>()? {
            Some(_) => Err(de::Error::custom(CHANGED)),
            None => Ok(()),
        }
    }
}

struct FillMap<'a>(&'a mut [(Node, Node)]);

impl<'de> Visitor<'de> for FillMap<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a mapping")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        for (key, value) in self.0 {
            map.next_key_seed(Fill(key))?
                .ok_or_else(|| de::Error::custom(CHANGED))?;
            map.next_value_seed(Fill(value))?;
        }
        match map.next_key::<IgnoredAny>()? {
            Some(_) => Err(de::Error::custom(CHANGED)),
            None => Ok(()),
        }
    }
}

/// The third pass: reads a value from the tree, as serde_yaml_ng would
/// from the file, except that a text field gets the text as written.
#[derive(Clone, Copy)]
struct Reader<'a> {
    node: &'a Node,
    /// Whether a value that may be anything is read as text: set where
    /// `lenient::TEXT` asks for it.
    text: bool,
}

impl<'a> Reader<'a> {
    fn new(node: &'a Node) -> Self {
        Self { node, text: false }
    }

    fn visit_seq<'de, V: Visitor<'de>>(
        self,
        items: &'a [Node],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let mut seq = SeqDeserializer::new(items.iter().map(|node| Reader { node, ..self }));
        let value = visitor.visit_seq(&mut seq)?;
        seq.end()?;
        Ok(value)
    }

    fn visit_map<'de, V: Visitor<'de>>(
        self,
        entries: &'a [(Node, Node)],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let mut map = MapDeserializer::new(entries.iter().map(|(key, value)| {
            (
                Reader { node: key, ..self },
                Reader {
                    node: value,
                    ..self
                },
            )
        }));
        let value = visitor.visit_map(&mut map)?;
        map.end()?;
        Ok(value)
    }
}

impl IntoDeserializer<'_, Error> for Reader<'_> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

/// Reads a scalar's resolved value with the `Value` method of the same
/// name. A sequence or a mapping is the wrong type.
macro_rules! typed {
    ($($method:ident($($arg:ident: $type:ty),*);)*) => {
        $(
            fn $method<V: Visitor<'de>>(self, $($arg: $type,)* visitor: V) -> Result<V::Value, Error> {
                match self.node {
                    Node::Scalar { value, .. } => value.clone().$method($($arg,)* visitor),
                    other => Err(de::Error::invalid_type(other.unexpected(), &visitor)),
                }
            }
        )*
    };
}

impl<'de> Deserializer<'de> for Reader<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Scalar { text, value } if self.text && !value.is_null() => {
                visitor.visit_str(text)
            }
            Node::Scalar { value, .. } => value.clone().deserialize_any(visitor),
            Node::Seq(items) => self.visit_seq(items, visitor),
            Node::Map(entries) => self.visit_map(entries, visitor),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Scalar { text, .. } => visitor.visit_str(text),
            other => Err(de::Error::invalid_type(other.unexpected(), &visitor)),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.node.is_null() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        let text = self.text || name == lenient::TEXT;
        visitor.visit_newtype_struct(Reader { text, ..self })
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Seq(items) => self.visit_seq(items, visitor),
            Node::Scalar { value, .. } => value.clone().deserialize_seq(visitor),
            Node::Map(_) => Err(de::Error::invalid_type(Unexpected::Map, &visitor)),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Map(entries) => self.visit_map(entries, visitor),
            Node::Scalar { value, .. } => value.clone().deserialize_map(visitor),
            Node::Seq(_) => Err(de::Error::invalid_type(Unexpected::Seq, &visitor)),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }

    typed! {
        deserialize_bool();
        deserialize_i8();
        deserialize_i16();
        deserialize_i32();
        deserialize_i64();
        deserialize_i128();
        deserialize_u8();
        deserialize_u16();
        deserialize_u32();
        deserialize_u64();
        deserialize_u128();
        deserialize_f32();
        deserialize_f64();
        deserialize_bytes();
        deserialize_byte_buf();
        deserialize_unit();
        deserialize_unit_struct(name: &'static str);
        deserialize_enum(name: &'static str, variants: &'static [&'static str]);
    }
}

#[cfg(test)]
mod tests {
    use super::from_str;
    use crate::model::{Environment, Request, Settings};

    #[test]
    fn scalars_where_text_is_expected_keep_their_text() {
        let env: Environment = from_str(
            "variables:\n  float: 1.0\n  hex: 0x10\n  exponent: 1e3\n  zeros: 007\n  octal: 0o17\n  plus: +1\n  half: .5\n  trailing: 1.50\n  yes: True\n  no: FALSE\n  tilde: ~\n  word: null\n  empty:\n  quoted: '1.0'\n  1.0: key\n",
        )
        .unwrap();
        let expected = [
            ("float", "1.0"),
            ("hex", "0x10"),
            ("exponent", "1e3"),
            ("zeros", "007"),
            ("octal", "0o17"),
            ("plus", "+1"),
            ("half", ".5"),
            ("trailing", "1.50"),
            ("yes", "True"),
            ("no", "FALSE"),
            ("tilde", ""),
            ("word", ""),
            ("empty", ""),
            ("quoted", "1.0"),
            ("1.0", "key"),
        ];
        for (name, value) in expected {
            assert_eq!(env.variables[name], value, "{name}");
        }
    }

    #[test]
    fn request_fields_keep_their_text() {
        let request: Request = from_str(
            "url: 1.0\nheaders:\n  - name: X-Version\n    value: 2.10\n  - name: X-Flag\n    value: True\n    enabled: false\npath_params:\n  id: 0x1F\ncaptures:\n  count: 1e3\nexamples:\n  - name: Seven\n    body: 7.0\n",
        )
        .unwrap();
        assert_eq!(request.url, "1.0");
        assert_eq!(request.headers[0].value, "2.10");
        assert_eq!(request.headers[1].value, "True");
        assert!(!request.headers[1].enabled);
        assert_eq!(request.path_params["id"], "0x1F");
        assert_eq!(request.captures["count"], "1e3");
        assert_eq!(request.examples[0].body, "7.0");
    }

    #[test]
    fn typed_fields_still_read_numbers_booleans_and_nulls() {
        let settings: Settings = from_str(
            "timeout_ms: 0x10\nmax_redirects: 3\nfollow_redirects: True\nverify_tls: false\ncookies: ~\n",
        )
        .unwrap();
        assert_eq!(settings.timeout_ms, Some(16));
        assert_eq!(settings.max_redirects, Some(3));
        assert_eq!(settings.follow_redirects, Some(true));
        assert_eq!(settings.verify_tls, Some(false));
        assert_eq!(settings.cookies, None);
        let error = from_str::<Settings>("timeout_ms: '10'\n").unwrap_err();
        assert!(error.to_string().contains("expected u64"), "{error}");
    }

    #[test]
    fn a_file_read_and_written_again_keeps_its_values() {
        let text = "variables:\n  version: 1.0\n  hex: 0x10\n  word: 'True'\n";
        let env: Environment = from_str(text).unwrap();
        let written = serde_yaml_ng::to_string(&env).unwrap();
        let back: Environment = from_str(&written).unwrap();
        assert_eq!(back, env);
        assert_eq!(serde_yaml_ng::to_string(&back).unwrap(), written);
        assert_eq!(back.variables["version"], "1.0");
        assert_eq!(back.variables["hex"], "0x10");
    }

    #[test]
    fn json_text_fields_still_take_numbers() {
        let env: Environment =
            serde_json::from_str(r#"{"variables": {"port": 3000, "debug": true, "none": null}}"#)
                .unwrap();
        assert_eq!(env.variables["port"], "3000");
        assert_eq!(env.variables["debug"], "true");
        assert_eq!(env.variables["none"], "");
    }

    #[test]
    fn aliases_and_tags_read_like_their_target() {
        let env: Environment =
            from_str("variables:\n  base: &v 1.10\n  copy: *v\n  tagged: !custom 0x10\n").unwrap();
        assert_eq!(env.variables["base"], "1.10");
        assert_eq!(env.variables["copy"], "1.10");
        assert_eq!(env.variables["tagged"], "0x10");
    }

    #[test]
    fn type_errors_keep_their_place_in_the_file() {
        let error = from_str::<Request>("url: x\nsettings:\n  timeout_ms: soon\n").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("settings.timeout_ms"), "{message}");
        assert!(message.contains("line 3"), "{message}");
    }
}
