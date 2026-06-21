//! Formatting: pretty-print, compact, and key sorting.

use anyhow::Result;
use serde_json::Value;

/// Options controlling how a [`Value`] is serialized.
#[derive(Debug, Clone, Copy, Default)]
pub struct FmtOptions {
    /// Emit compact (single-line) JSON instead of pretty-printed.
    pub compact: bool,
    /// Recursively sort object keys alphabetically before serializing.
    pub sort_keys: bool,
}

/// Serialize a [`Value`] to a string according to `opts`.
///
/// With `preserve_order` enabled, key order is the input order unless
/// `sort_keys` is set, in which case keys are sorted recursively.
pub fn format_value(value: &Value, opts: FmtOptions) -> Result<String> {
    let owned;
    let v = if opts.sort_keys {
        owned = sorted(value);
        &owned
    } else {
        value
    };

    let s = if opts.compact {
        serde_json::to_string(v)?
    } else {
        serde_json::to_string_pretty(v)?
    };
    Ok(s)
}

/// Return a deep copy of `value` with every object's keys sorted
/// alphabetically (recursively into arrays and nested objects).
pub fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k.clone(), sorted(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_vs_pretty() {
        let v: Value = serde_json::from_str(r#"{"a":1,"b":[2,3]}"#).unwrap();

        let compact = format_value(
            &v,
            FmtOptions {
                compact: true,
                sort_keys: false,
            },
        )
        .unwrap();
        assert_eq!(compact, r#"{"a":1,"b":[2,3]}"#);
        assert!(!compact.contains('\n'));

        let pretty = format_value(
            &v,
            FmtOptions {
                compact: false,
                sort_keys: false,
            },
        )
        .unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("  ")); // indentation present
                                        // pretty should round-trip back to the same value
        let reparsed: Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(reparsed, v);
    }

    #[test]
    fn preserve_order_holds() {
        // Keys deliberately out of alphabetical order. With preserve_order,
        // they must come out in INPUT order, not sorted.
        let v: Value = serde_json::from_str(r#"{"zebra":1,"apple":2,"mango":3}"#).unwrap();
        let compact = format_value(
            &v,
            FmtOptions {
                compact: true,
                sort_keys: false,
            },
        )
        .unwrap();
        assert_eq!(compact, r#"{"zebra":1,"apple":2,"mango":3}"#);
    }

    #[test]
    fn sort_keys_sorts_recursively() {
        let v: Value = serde_json::from_str(r#"{"zebra":1,"apple":{"yak":1,"ant":2}}"#).unwrap();
        let out = format_value(
            &v,
            FmtOptions {
                compact: true,
                sort_keys: true,
            },
        )
        .unwrap();
        assert_eq!(out, r#"{"apple":{"ant":2,"yak":1},"zebra":1}"#);
    }
}
