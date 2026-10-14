//! Data files that drive a run: one iteration per row.
//!
//! A CSV file's first row names the columns. A JSON file is an array of
//! objects. Each row's values are available as variables, and through
//! `gm.iterationData` in scripts.

use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{Error, Result};

/// One row of a data file.
pub type Row = Map<String, Value>;

/// Reads a CSV or JSON data file, chosen by its extension.
pub fn read(path: &Path) -> Result<Vec<Row>> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let is_json = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    if is_json {
        parse_json(&text)
    } else {
        parse_csv(&text)
    }
    .map_err(|message| Error::Invalid(format!("{}: {message}", path.display())))
}

/// Reads rows from a JSON array of objects.
pub fn parse_json(text: &str) -> Result<Vec<Row>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let Value::Array(items) = value else {
        return Err("expected an array of objects".into());
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match item {
            Value::Object(row) => Ok(row),
            _ => Err(format!("item {} is not an object", i + 1)),
        })
        .collect()
}

/// Reads rows from CSV text whose first row names the columns.
pub fn parse_csv(text: &str) -> Result<Vec<Row>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(false)
        .trim(csv::Trim::Headers)
        .from_reader(text.trim_start_matches('\u{feff}').as_bytes());
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| e.to_string())?;
        let row = headers
            .iter()
            .zip(record.iter())
            .map(|(name, value)| (name.to_owned(), Value::String(value.to_owned())))
            .collect();
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_rows_use_the_header_row() {
        let rows =
            parse_csv("\u{feff}name, email\nAda,ada@example.com\n\"Lovelace, A\",\"a\"\"b\"\n")
                .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["email"], "ada@example.com");
        assert_eq!(rows[1]["name"], "Lovelace, A");
        assert_eq!(rows[1]["email"], "a\"b");
        assert!(parse_csv("a,b\n1\n").is_err());
    }

    #[test]
    fn json_rows_keep_their_types() {
        let rows = parse_json(r#"[{"id": 1, "tags": ["x"]}, {"id": 2}]"#).unwrap();
        assert_eq!(rows[0]["id"], 1);
        assert_eq!(rows[0]["tags"][0], "x");
        assert_eq!(
            parse_json(r#"{"id": 1}"#).unwrap_err(),
            "expected an array of objects"
        );
        assert_eq!(parse_json("[1]").unwrap_err(), "item 1 is not an object");
    }
}
