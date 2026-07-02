//! Convert an array of JSON objects into CSV.
//!
//! The column set is the union of every object's keys, in **first-seen order**
//! (serde_json is built with `preserve_order`, so this mirrors the input): the
//! first object contributes its keys in order, then each later object appends
//! any keys not yet seen. This keeps the header stable and predictable instead
//! of sorting alphabetically.
//!
//! A single top-level object is treated as a one-row table. Values are rendered
//! as CSV fields per RFC 4180: `null` becomes an empty field, strings are
//! emitted verbatim, numbers/booleans use their JSON form, and nested
//! objects/arrays are serialized as compact JSON. Fields containing a comma,
//! quote, CR, or LF are wrapped in double quotes with embedded quotes doubled.

use anyhow::{bail, Result};
use serde_json::{Map, Value};

/// Render `value` (an array of objects, or a single object) as an RFC 4180 CSV
/// string. Lines are separated by `\n`; there is no trailing newline (the CLI
/// wrapper adds one, matching every other subcommand).
pub fn to_csv(value: &Value) -> Result<String> {
    // Accept either an array of objects or a lone object (one row).
    let rows: Vec<&Map<String, Value>> = match value {
        Value::Array(arr) => {
            let mut rows = Vec::with_capacity(arr.len());
            for (i, item) in arr.iter().enumerate() {
                match item {
                    Value::Object(m) => rows.push(m),
                    other => bail!(
                        "to-csv expects an array of objects; element {} is {}",
                        i,
                        type_name(other)
                    ),
                }
            }
            rows
        }
        Value::Object(m) => vec![m],
        other => bail!(
            "to-csv expects an array of objects (or a single object); got {}",
            type_name(other)
        ),
    };

    // Union of keys in first-seen order.
    let mut columns: Vec<String> = Vec::new();
    for row in &rows {
        for key in row.keys() {
            if !columns.iter().any(|c| c == key) {
                columns.push(key.clone());
            }
        }
    }

    let mut out = String::new();
    push_record(&mut out, columns.iter().map(|c| escape_field(c)));
    for row in &rows {
        out.push('\n');
        push_record(
            &mut out,
            columns.iter().map(|col| match row.get(col) {
                Some(v) => escape_field(&cell(v)),
                None => String::new(),
            }),
        );
    }
    Ok(out)
}

/// Join already-escaped fields with commas onto `out`.
fn push_record(out: &mut String, fields: impl Iterator<Item = String>) {
    let mut first = true;
    for f in fields {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&f);
    }
}

/// Render a single JSON value as its unescaped CSV cell text.
fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        // Nested containers have no flat CSV form; keep them as compact JSON.
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Quote a field per RFC 4180 if it contains a delimiter, quote, or newline.
fn escape_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_of_objects_basic() {
        let v: Value =
            serde_json::from_str(r#"[{"name":"Ann","age":30},{"name":"Bob","age":25}]"#).unwrap();
        assert_eq!(to_csv(&v).unwrap(), "name,age\nAnn,30\nBob,25");
    }

    #[test]
    fn preserves_key_order_not_alpha() {
        let v: Value = serde_json::from_str(r#"[{"zebra":1,"apple":2}]"#).unwrap();
        assert_eq!(to_csv(&v).unwrap(), "zebra,apple\n1,2");
    }

    #[test]
    fn union_of_keys_first_seen_order() {
        // Second row introduces `c`; missing cells are empty.
        let v: Value = serde_json::from_str(r#"[{"a":1,"b":2},{"a":3,"c":4}]"#).unwrap();
        assert_eq!(to_csv(&v).unwrap(), "a,b,c\n1,2,\n3,,4");
    }

    #[test]
    fn single_object_is_one_row() {
        let v: Value = serde_json::from_str(r#"{"x":1,"y":2}"#).unwrap();
        assert_eq!(to_csv(&v).unwrap(), "x,y\n1,2");
    }

    #[test]
    fn quotes_fields_needing_escaping() {
        let v: Value =
            serde_json::from_str(r#"[{"a":"x,y","b":"he said \"hi\"","c":"line1\nline2"}]"#)
                .unwrap();
        assert_eq!(
            to_csv(&v).unwrap(),
            "a,b,c\n\"x,y\",\"he said \"\"hi\"\"\",\"line1\nline2\""
        );
    }

    #[test]
    fn null_becomes_empty_nested_becomes_json() {
        let v: Value =
            serde_json::from_str(r#"[{"a":null,"b":{"k":1},"c":[1,2]}]"#).unwrap();
        assert_eq!(to_csv(&v).unwrap(), "a,b,c\n,\"{\"\"k\"\":1}\",\"[1,2]\"");
    }

    #[test]
    fn empty_array_is_header_only_empty() {
        let v: Value = serde_json::from_str("[]").unwrap();
        assert_eq!(to_csv(&v).unwrap(), "");
    }

    #[test]
    fn non_object_element_errors() {
        let v: Value = serde_json::from_str(r#"[{"a":1}, 5]"#).unwrap();
        assert!(to_csv(&v).is_err());
    }

    #[test]
    fn scalar_input_errors() {
        assert!(to_csv(&Value::from(42)).is_err());
    }
}
