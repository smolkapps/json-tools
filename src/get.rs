//! Path extraction and key listing.
//!
//! Supports two path syntaxes for `get`:
//! - JSON Pointer (RFC 6901): `/a/b/0` (leading slash; `~1` -> `/`, `~0` -> `~`).
//! - Simple dot path: `a.b.0` (segments split on `.`; numeric segments index
//!   into arrays).

use anyhow::{anyhow, Result};
use serde_json::Value;

/// Resolve `path` against `root`, returning a reference to the located value.
///
/// If `path` starts with `/` (or is empty) it is treated as a JSON Pointer;
/// otherwise it is treated as a simple dot path.
pub fn get<'a>(root: &'a Value, path: &str) -> Result<&'a Value> {
    if path.is_empty() || path.starts_with('/') {
        get_pointer(root, path)
    } else {
        get_dot(root, path)
    }
}

/// Resolve a JSON Pointer (RFC 6901) against `root`.
pub fn get_pointer<'a>(root: &'a Value, pointer: &str) -> Result<&'a Value> {
    root.pointer(pointer)
        .ok_or_else(|| anyhow!("no value at JSON Pointer `{}`", pointer))
}

/// Resolve a simple dot path (`a.b.0`) against `root`.
pub fn get_dot<'a>(root: &'a Value, path: &str) -> Result<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        cur = step(cur, seg)
            .ok_or_else(|| anyhow!("no value at dot path `{}` (failed at `{}`)", path, seg))?;
    }
    Ok(cur)
}

/// Take a single step into `value` by key (object) or index (array).
fn step<'a>(value: &'a Value, seg: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map.get(seg),
        Value::Array(arr) => seg.parse::<usize>().ok().and_then(|i| arr.get(i)),
        _ => None,
    }
}

/// List the keys of the object located at `path` (or of `root` when `path` is
/// `None`). Returns an error if the located value is not an object.
///
/// Key order follows the object's stored order (input order, thanks to
/// `preserve_order`).
pub fn keys(root: &Value, path: Option<&str>) -> Result<Vec<String>> {
    let target = match path {
        Some(p) => get(root, p)?,
        None => root,
    };
    match target {
        Value::Object(map) => Ok(map.keys().cloned().collect()),
        other => Err(anyhow!(
            "value is not an object (it is {}); cannot list keys",
            type_name(other)
        )),
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

    fn sample() -> Value {
        serde_json::from_str(r#"{"a":{"b":[{"c":10},{"c":20}]},"name":"x","list":[1,2,3]}"#)
            .unwrap()
    }

    #[test]
    fn get_by_pointer() {
        let v = sample();
        assert_eq!(get(&v, "/a/b/0/c").unwrap(), &Value::from(10));
        assert_eq!(get(&v, "/a/b/1/c").unwrap(), &Value::from(20));
        assert_eq!(get(&v, "/name").unwrap(), &Value::from("x"));
        assert_eq!(get(&v, "/list/2").unwrap(), &Value::from(3));
    }

    #[test]
    fn get_by_dot_path() {
        let v = sample();
        assert_eq!(get(&v, "a.b.0.c").unwrap(), &Value::from(10));
        assert_eq!(get(&v, "a.b.1.c").unwrap(), &Value::from(20));
        assert_eq!(get(&v, "name").unwrap(), &Value::from("x"));
        assert_eq!(get(&v, "list.2").unwrap(), &Value::from(3));
    }

    #[test]
    fn get_pointer_and_dot_agree() {
        let v = sample();
        assert_eq!(get(&v, "/a/b/0/c").unwrap(), get(&v, "a.b.0.c").unwrap());
    }

    #[test]
    fn get_missing_errors() {
        let v = sample();
        assert!(get(&v, "/a/nope").is_err());
        assert!(get(&v, "a.nope").is_err());
        assert!(get(&v, "list.99").is_err());
    }

    #[test]
    fn keys_top_level_in_input_order() {
        let v = sample();
        // Input order is a, name, list — NOT alphabetical.
        assert_eq!(keys(&v, None).unwrap(), vec!["a", "name", "list"]);
    }

    #[test]
    fn keys_at_path() {
        let v = sample();
        assert_eq!(keys(&v, Some("a")).unwrap(), vec!["b"]);
    }

    #[test]
    fn keys_non_object_errors() {
        let v = sample();
        assert!(keys(&v, Some("list")).is_err());
    }
}
