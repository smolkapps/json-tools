//! Convert an array of JSON objects into CSV.
//!
//! The column set is the union of every object's keys, in **first-seen order**
//! (serde_json is built with `preserve_order`, so this mirrors the input): the
//! first object contributes its keys in order, then each later object appends
//! any keys not yet seen. This keeps the header stable and predictable instead
//! of sorting alphabetically.
//!
//! A single top-level object is treated as a one-row table. `null` becomes an
//! empty field, strings are emitted verbatim, numbers/booleans use their JSON
//! form, and nested objects/arrays are serialized as compact JSON. Field
//! quoting follows RFC 4180's rules — fields containing a comma, quote, CR, or
//! LF are wrapped in double quotes with embedded quotes doubled — but records
//! are separated by a bare `\n` (LF), not RFC 4180's CRLF, matching the
//! Unix-newline output of every other subcommand.
//!
//! Numbers outside the i64/u64 range are parsed by serde_json as `f64`, so very
//! large integers may render in scientific notation (e.g. `1e30`).

use anyhow::{bail, Result};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Options controlling CSV rendering.
#[derive(Clone, Copy, Debug, Default)]
pub struct CsvOptions {
    /// Prefix any field beginning with `=`, `+`, `-`, or `@` with a single
    /// quote so spreadsheet apps treat it as text rather than a formula
    /// (CSV/formula-injection defense). Off by default.
    pub escape_formulas: bool,
}

/// Render `value` (an array of objects, or a single object) as a CSV string.
/// Fields are quoted per RFC 4180's quoting rules, but records are separated by
/// a bare `\n` (LF), not RFC 4180's CRLF. There is no trailing newline (the CLI
/// wrapper adds one, matching every other subcommand).
pub fn to_csv(value: &Value, opts: CsvOptions) -> Result<String> {
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

    // Union of keys in first-seen order. A HashSet alongside the Vec keeps the
    // membership test O(1), so building the column set is O(K) overall rather
    // than the O(K^2) of a linear scan per key.
    let mut columns: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for row in &rows {
        for key in row.keys() {
            if seen.insert(key.as_str()) {
                columns.push(key.clone());
            }
        }
    }

    let mut out = String::new();
    push_record(
        &mut out,
        columns.iter().map(|c| render_field(c, opts.escape_formulas)),
    );
    for row in &rows {
        out.push('\n');
        push_record(
            &mut out,
            columns.iter().map(|col| match row.get(col) {
                Some(v) => render_field(&cell(v), opts.escape_formulas),
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

/// Render a cell's text into its final CSV field: optionally defuse formula
/// injection, then apply RFC 4180 quoting.
fn render_field(s: &str, escape_formulas: bool) -> String {
    if escape_formulas && s.starts_with(['=', '+', '-', '@']) {
        // Prefix with a single quote so spreadsheets treat the cell as text.
        // Quoting runs afterwards, so the `'` ends up inside any wrapping.
        escape_field(&format!("'{s}"))
    } else {
        escape_field(s)
    }
}

/// Quote a field per RFC 4180's rules if it contains a delimiter, quote, CR, or
/// LF; embedded quotes are doubled.
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

    fn csv(v: &Value) -> String {
        to_csv(v, CsvOptions::default()).unwrap()
    }

    #[test]
    fn array_of_objects_basic() {
        let v: Value =
            serde_json::from_str(r#"[{"name":"Ann","age":30},{"name":"Bob","age":25}]"#).unwrap();
        assert_eq!(csv(&v), "name,age\nAnn,30\nBob,25");
    }

    #[test]
    fn preserves_key_order_not_alpha() {
        let v: Value = serde_json::from_str(r#"[{"zebra":1,"apple":2}]"#).unwrap();
        assert_eq!(csv(&v), "zebra,apple\n1,2");
    }

    #[test]
    fn union_of_keys_first_seen_order() {
        // Second row introduces `c`; missing cells are empty.
        let v: Value = serde_json::from_str(r#"[{"a":1,"b":2},{"a":3,"c":4}]"#).unwrap();
        assert_eq!(csv(&v), "a,b,c\n1,2,\n3,,4");
    }

    #[test]
    fn single_object_is_one_row() {
        let v: Value = serde_json::from_str(r#"{"x":1,"y":2}"#).unwrap();
        assert_eq!(csv(&v), "x,y\n1,2");
    }

    #[test]
    fn quotes_fields_needing_escaping() {
        let v: Value =
            serde_json::from_str(r#"[{"a":"x,y","b":"he said \"hi\"","c":"line1\nline2"}]"#)
                .unwrap();
        assert_eq!(
            csv(&v),
            "a,b,c\n\"x,y\",\"he said \"\"hi\"\"\",\"line1\nline2\""
        );
    }

    #[test]
    fn quotes_field_containing_carriage_return() {
        // A bare CR must also trigger quoting.
        let v: Value = serde_json::from_str(r#"[{"a":"line1\rline2"}]"#).unwrap();
        assert_eq!(csv(&v), "a\n\"line1\rline2\"");
    }

    #[test]
    fn null_becomes_empty_nested_becomes_json() {
        let v: Value =
            serde_json::from_str(r#"[{"a":null,"b":{"k":1},"c":[1,2]}]"#).unwrap();
        assert_eq!(csv(&v), "a,b,c\n,\"{\"\"k\"\":1}\",\"[1,2]\"");
    }

    #[test]
    fn empty_array_produces_empty_output() {
        let v: Value = serde_json::from_str("[]").unwrap();
        assert_eq!(csv(&v), "");
    }

    #[test]
    fn escape_formulas_off_by_default() {
        let v: Value = serde_json::from_str(r#"[{"a":"=1+2","b":"-3"}]"#).unwrap();
        assert_eq!(csv(&v), "a,b\n=1+2,-3");
    }

    #[test]
    fn escape_formulas_prefixes_dangerous_cells() {
        // Each of = + - @ gets a leading single quote; other cells untouched.
        let v: Value = serde_json::from_str(
            r#"[{"eq":"=1+1","plus":"+2","minus":"-3","at":"@cmd","safe":"ok"}]"#,
        )
        .unwrap();
        let opts = CsvOptions {
            escape_formulas: true,
        };
        assert_eq!(
            to_csv(&v, opts).unwrap(),
            "eq,plus,minus,at,safe\n'=1+1,'+2,'-3,'@cmd,ok"
        );
    }

    #[test]
    fn escape_formulas_then_quotes_when_needed() {
        // A dangerous cell that also needs quoting: prefix first, then quote,
        // so the single quote lands inside the wrapping quotes.
        let v: Value = serde_json::from_str(r#"[{"a":"=1,2"}]"#).unwrap();
        let opts = CsvOptions {
            escape_formulas: true,
        };
        assert_eq!(to_csv(&v, opts).unwrap(), "a\n\"'=1,2\"");
    }

    #[test]
    fn escape_formulas_applies_to_header() {
        // A column name beginning with a formula char is defused too.
        let v: Value = serde_json::from_str(r#"[{"=danger":1}]"#).unwrap();
        let opts = CsvOptions {
            escape_formulas: true,
        };
        assert_eq!(to_csv(&v, opts).unwrap(), "'=danger\n1");
    }

    #[test]
    fn non_object_element_errors() {
        let v: Value = serde_json::from_str(r#"[{"a":1}, 5]"#).unwrap();
        assert!(to_csv(&v, CsvOptions::default()).is_err());
    }

    #[test]
    fn scalar_input_errors() {
        assert!(to_csv(&Value::from(42), CsvOptions::default()).is_err());
    }
}
