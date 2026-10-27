//! Messages as JSON: reading what people write, showing what servers send,
//! and example messages to start from.

use std::collections::HashSet;

use prost_reflect::{DynamicMessage, Kind, MessageDescriptor, SerializeOptions};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};

/// Message fields deeper than this are left empty in examples, which stops
/// recursive types.
const EXAMPLE_DEPTH: usize = 4;

/// A message as JSON in the canonical protobuf mapping, with fields at
/// their defaults included so checks can find them.
pub(crate) fn to_json(message: &DynamicMessage) -> Result<Value, String> {
    message
        .serialize_with_options(
            serde_json::value::Serializer,
            &SerializeOptions::new().skip_default_fields(false),
        )
        .map_err(|e| e.to_string())
}

/// Reads JSON values written one after another, as streams from the
/// client take them. Empty text gives no values.
pub(crate) fn split_messages(text: &str) -> Result<Vec<Value>> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .map(|value| {
            value.map_err(|e| Error::Invalid(format!("The message is not valid JSON: {e}")))
        })
        .collect()
}

/// A JSON value as a message of the given type.
pub(crate) fn from_json(descriptor: &MessageDescriptor, value: &Value) -> Result<DynamicMessage> {
    DynamicMessage::deserialize(descriptor.clone(), value).map_err(|e| {
        Error::Invalid(format!(
            "The message does not fit {}: {e}",
            descriptor.full_name()
        ))
    })
}

/// An example of a message: every field at a plain value, one item in each
/// list and map, and the first field of each `oneof`.
pub fn example(descriptor: &MessageDescriptor) -> Value {
    example_message(descriptor, 0)
}

fn example_message(descriptor: &MessageDescriptor, depth: usize) -> Value {
    if let Some(value) = well_known(descriptor.full_name()) {
        return value;
    }
    let mut map = Map::new();
    if depth >= EXAMPLE_DEPTH {
        return Value::Object(map);
    }
    let mut oneofs = HashSet::new();
    for field in descriptor.fields() {
        if let Some(oneof) = field.containing_oneof()
            && !oneof.is_synthetic()
            && !oneofs.insert(oneof.full_name().to_owned())
        {
            continue;
        }
        let kind = field.kind();
        let value = if field.is_map() {
            let entry = kind.as_message().expect("map fields are messages");
            let key = match entry.map_entry_key_field().kind() {
                Kind::String => "key".to_owned(),
                Kind::Bool => "true".to_owned(),
                _ => "0".to_owned(),
            };
            let mut one = Map::new();
            one.insert(
                key,
                example_value(&entry.map_entry_value_field().kind(), depth),
            );
            Value::Object(one)
        } else if field.is_list() {
            Value::Array(vec![example_value(&kind, depth)])
        } else {
            example_value(&kind, depth)
        };
        map.insert(field.json_name().to_owned(), value);
    }
    Value::Object(map)
}

fn example_value(kind: &Kind, depth: usize) -> Value {
    match kind {
        Kind::Double | Kind::Float => json!(0.0),
        Kind::Int32
        | Kind::Int64
        | Kind::Uint32
        | Kind::Uint64
        | Kind::Sint32
        | Kind::Sint64
        | Kind::Fixed32
        | Kind::Fixed64
        | Kind::Sfixed32
        | Kind::Sfixed64 => json!(0),
        Kind::Bool => json!(false),
        Kind::String | Kind::Bytes => json!(""),
        Kind::Enum(descriptor) => json!(descriptor.default_value().name()),
        Kind::Message(descriptor) => example_message(descriptor, depth + 1),
    }
}

/// Examples for the well-known types, in their JSON forms.
fn well_known(name: &str) -> Option<Value> {
    Some(match name {
        "google.protobuf.Timestamp" => json!("1970-01-01T00:00:00Z"),
        "google.protobuf.Duration" => json!("1s"),
        "google.protobuf.FieldMask" => json!(""),
        "google.protobuf.Empty" | "google.protobuf.Struct" => json!({}),
        "google.protobuf.ListValue" => json!([]),
        "google.protobuf.Value" | "google.protobuf.Any" => Value::Null,
        "google.protobuf.StringValue" | "google.protobuf.BytesValue" => json!(""),
        "google.protobuf.BoolValue" => json!(false),
        "google.protobuf.DoubleValue"
        | "google.protobuf.FloatValue"
        | "google.protobuf.Int32Value"
        | "google.protobuf.Int64Value"
        | "google.protobuf.UInt32Value"
        | "google.protobuf.UInt64Value" => json!(0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(source: &str, name: &str) -> MessageDescriptor {
        use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};

        struct One(String);
        impl FileResolver for One {
            fn open_file(&self, name: &str) -> Result<File, protox::Error> {
                if name == "test.proto" {
                    File::from_source(name, &self.0)
                } else {
                    Err(protox::Error::file_not_found(name))
                }
            }
        }
        let mut resolver = ChainFileResolver::new();
        resolver.add(One(source.to_owned()));
        resolver.add(GoogleFileResolver::new());
        let mut compiler = protox::Compiler::with_file_resolver(resolver);
        compiler.open_file("test.proto").unwrap();
        compiler
            .descriptor_pool()
            .get_message_by_name(name)
            .unwrap()
    }

    const SOURCE: &str = r#"
        syntax = "proto3";
        package t;
        import "google/protobuf/timestamp.proto";
        enum Mood { MOOD_UNSPECIFIED = 0; HAPPY = 1; }
        message Node { string label = 1; Node child = 2; }
        message Order {
          string id = 1;
          int64 total_cents = 2;
          repeated string tags = 3;
          map<string, int32> counts = 4;
          Mood mood = 5;
          oneof pay { string card = 6; string voucher = 7; }
          optional bool gift = 8;
          google.protobuf.Timestamp at = 9;
          Node tree = 10;
        }
    "#;

    #[test]
    fn examples_show_every_field_once() {
        let example = example(&descriptor(SOURCE, "t.Order"));
        assert_eq!(example["id"], "");
        assert_eq!(example["totalCents"], 0);
        assert_eq!(example["tags"], json!([""]));
        assert_eq!(example["counts"], json!({"key": 0}));
        assert_eq!(example["mood"], "MOOD_UNSPECIFIED");
        assert_eq!(example["card"], "");
        assert!(example.get("voucher").is_none());
        assert_eq!(example["gift"], false);
        assert_eq!(example["at"], "1970-01-01T00:00:00Z");
        assert_eq!(example["tree"]["child"]["child"]["child"], json!({}));
    }

    #[test]
    fn examples_read_back_as_messages() {
        let order = descriptor(SOURCE, "t.Order");
        let message = from_json(&order, &example(&order)).unwrap();
        let back = to_json(&message).unwrap();
        assert_eq!(back["mood"], "MOOD_UNSPECIFIED");
        assert_eq!(back["totalCents"], "0");
    }

    #[test]
    fn messages_follow_one_another() {
        let values = split_messages("{\"a\": 1}\n{\"a\": 2} {}").unwrap();
        assert_eq!(values.len(), 3);
        assert!(split_messages("  \n").unwrap().is_empty());
        assert!(split_messages("{\"a\": ").is_err());
        let order = descriptor(SOURCE, "t.Order");
        let error = from_json(&order, &json!({"nope": 1})).unwrap_err();
        assert!(
            error.to_string().contains("does not fit t.Order"),
            "{error}"
        );
    }
}
