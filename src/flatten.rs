//! Flatten a nested JSON value into a single-level object whose keys are
//! path expressions, and the inverse (`unflatten`).
//!
//! Key syntax:
//! - object members are joined with `.`   -> `a.b`
//! - array elements use bracket indices    -> `a[0]`, `a[0].b`
//!
//! Scalars (null, bool, number, string) are leaves. **Empty** objects and
//! empty arrays are also emitted as leaves (as `{}` / `[]`) so that they
//! survive a flatten -> unflatten round-trip; without this they would simply
//! vanish, since they contribute no leaf keys.

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

/// Flatten `value` into a flat object mapping path strings to scalar (or
/// empty-container) leaves.
///
/// A top-level scalar flattens to the single key `""` (empty string).
pub fn flatten(value: &Value) -> Value {
    let mut out = Map::new();
    flatten_into(value, String::new(), &mut out);
    Value::Object(out)
}

fn flatten_into(value: &Value, prefix: String, out: &mut Map<String, Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    escape_key(k)
                } else {
                    format!("{}.{}", prefix, escape_key(k))
                };
                flatten_into(v, key, out);
            }
        }
        Value::Array(arr) if !arr.is_empty() => {
            for (i, v) in arr.iter().enumerate() {
                let key = format!("{}[{}]", prefix, i);
                flatten_into(v, key, out);
            }
        }
        // Leaf: scalar, empty object, or empty array.
        leaf => {
            out.insert(prefix, leaf.clone());
        }
    }
}

/// When an object key itself contains `.` or `[`, escaping keeps the flat key
/// unambiguous. We wrap such keys in the bracket-quote form `['the.key']`.
fn escape_key(k: &str) -> String {
    if k.contains('.') || k.contains('[') || k.contains(']') {
        format!("['{}']", k.replace('\'', "\\'"))
    } else {
        k.to_string()
    }
}

/// Rebuild a nested value from a flat object produced by [`flatten`].
///
/// Errors if `flat` is not an object, or if two keys imply conflicting
/// container types at the same path (e.g. `a.b` and `a[0]`).
pub fn unflatten(flat: &Value) -> Result<Value> {
    let map = flat
        .as_object()
        .ok_or_else(|| anyhow!("unflatten input must be a JSON object"))?;

    // Top-level scalar case: a lone empty-string key holds the whole value.
    if map.len() == 1 {
        if let Some(v) = map.get("") {
            return Ok(v.clone());
        }
    }

    let mut root = Value::Null;
    for (key, leaf) in map {
        let tokens = parse_key(key)?;
        insert_tokens(&mut root, &tokens, leaf.clone())?;
    }
    Ok(root)
}

/// A single step in a parsed flat key.
#[derive(Debug, PartialEq, Eq)]
enum Token {
    Key(String),
    Index(usize),
}

/// Parse a flat key like `a.b[0].c` or `['weird.key'][2]` into tokens.
fn parse_key(key: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = key.chars().collect();
    let mut i = 0;
    let n = chars.len();

    // Leading dot is not expected, but a leading plain segment is.
    while i < n {
        match chars[i] {
            '.' => {
                i += 1; // separator before a plain key segment
            }
            '[' => {
                // Either a quoted key ['...'] or a numeric index [0].
                i += 1;
                if i < n && chars[i] == '\'' {
                    // quoted key
                    i += 1;
                    let mut s = String::new();
                    while i < n && chars[i] != '\'' {
                        if chars[i] == '\\' && i + 1 < n {
                            i += 1;
                            s.push(chars[i]);
                        } else {
                            s.push(chars[i]);
                        }
                        i += 1;
                    }
                    if i >= n || chars[i] != '\'' {
                        bail!("unterminated quoted key in `{}`", key);
                    }
                    i += 1; // closing quote
                    if i >= n || chars[i] != ']' {
                        bail!("expected `]` after quoted key in `{}`", key);
                    }
                    i += 1; // closing bracket
                    tokens.push(Token::Key(s));
                } else {
                    // numeric index
                    let mut num = String::new();
                    while i < n && chars[i] != ']' {
                        num.push(chars[i]);
                        i += 1;
                    }
                    if i >= n {
                        bail!("unterminated `[` in `{}`", key);
                    }
                    i += 1; // closing bracket
                    let idx: usize = num
                        .parse()
                        .map_err(|_| anyhow!("invalid array index `[{}]` in `{}`", num, key))?;
                    tokens.push(Token::Index(idx));
                }
            }
            _ => {
                // plain key segment up to next '.' or '['
                let mut s = String::new();
                while i < n && chars[i] != '.' && chars[i] != '[' {
                    s.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token::Key(s));
            }
        }
    }

    if tokens.is_empty() {
        bail!("empty flat key");
    }
    Ok(tokens)
}

/// Insert `leaf` into `node` following `tokens`, creating intermediate
/// objects/arrays as needed.
fn insert_tokens(node: &mut Value, tokens: &[Token], leaf: Value) -> Result<()> {
    let (head, rest) = tokens.split_first().expect("non-empty tokens");

    match head {
        Token::Key(k) => {
            if node.is_null() {
                *node = Value::Object(Map::new());
            }
            let map = node
                .as_object_mut()
                .ok_or_else(|| anyhow!("conflicting types: expected object for key `{}`", k))?;
            if rest.is_empty() {
                map.insert(k.clone(), leaf);
            } else {
                let child = map.entry(k.clone()).or_insert(Value::Null);
                insert_tokens(child, rest, leaf)?;
            }
        }
        Token::Index(idx) => {
            if node.is_null() {
                *node = Value::Array(Vec::new());
            }
            let arr = node
                .as_array_mut()
                .ok_or_else(|| anyhow!("conflicting types: expected array for index [{}]", idx))?;
            if arr.len() <= *idx {
                arr.resize(*idx + 1, Value::Null);
            }
            if rest.is_empty() {
                arr[*idx] = leaf;
            } else {
                insert_tokens(&mut arr[*idx], rest, leaf)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested() -> Value {
        serde_json::from_str(
            r#"{
                "a": {"b": [{"c": 1}, {"c": 2}]},
                "name": "widget",
                "tags": ["x", "y"],
                "meta": {"ok": true, "n": null},
                "empty_obj": {},
                "empty_arr": []
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn flatten_produces_expected_keys() {
        let flat = flatten(&nested());
        let m = flat.as_object().unwrap();
        assert_eq!(m.get("a.b[0].c").unwrap(), &Value::from(1));
        assert_eq!(m.get("a.b[1].c").unwrap(), &Value::from(2));
        assert_eq!(m.get("name").unwrap(), &Value::from("widget"));
        assert_eq!(m.get("tags[0]").unwrap(), &Value::from("x"));
        assert_eq!(m.get("tags[1]").unwrap(), &Value::from("y"));
        assert_eq!(m.get("meta.ok").unwrap(), &Value::from(true));
        assert_eq!(m.get("meta.n").unwrap(), &Value::Null);
        // empty containers preserved as leaves
        assert_eq!(m.get("empty_obj").unwrap(), &serde_json::json!({}));
        assert_eq!(m.get("empty_arr").unwrap(), &serde_json::json!([]));
    }

    #[test]
    fn flatten_unflatten_round_trips() {
        let original = nested();
        let flat = flatten(&original);
        let restored = unflatten(&flat).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn round_trip_deeply_nested() {
        let original: Value =
            serde_json::from_str(r#"{"x":[[1,2],[3,[4,5]]],"y":{"z":{"w":[{"q":"deep"}]}}}"#)
                .unwrap();
        let restored = unflatten(&flatten(&original)).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn top_level_scalar_round_trips() {
        let original = Value::from(42);
        let flat = flatten(&original);
        // single key "" -> the scalar
        assert_eq!(flat.as_object().unwrap().get("").unwrap(), &Value::from(42));
        assert_eq!(unflatten(&flat).unwrap(), original);
    }

    #[test]
    fn keys_with_dots_escaped_round_trip() {
        let original: Value = serde_json::from_str(r#"{"a.b":1,"normal":2,"c[0]":3}"#).unwrap();
        let flat = flatten(&original);
        let restored = unflatten(&flat).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn unflatten_non_object_errors() {
        assert!(unflatten(&Value::from(5)).is_err());
    }
}
