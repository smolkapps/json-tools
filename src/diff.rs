//! Structural diff between two JSON values.
//!
//! Walks both trees in parallel and reports paths (as JSON Pointers) that were
//! added, removed, or changed. Objects are compared key-by-key; arrays are
//! compared index-by-index (length differences show up as added/removed
//! trailing indices).

use serde_json::Value;

/// One difference between two JSON values, located by a JSON Pointer `path`.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Present only in the right-hand value.
    Added { path: String, value: Value },
    /// Present only in the left-hand value.
    Removed { path: String, value: Value },
    /// Present in both but with different values.
    Changed {
        path: String,
        from: Value,
        to: Value,
    },
}

impl Change {
    /// The JSON Pointer path this change is located at.
    pub fn path(&self) -> &str {
        match self {
            Change::Added { path, .. }
            | Change::Removed { path, .. }
            | Change::Changed { path, .. } => path,
        }
    }
}

/// Compute the list of changes turning `a` (left) into `b` (right).
///
/// Returned changes are ordered deterministically: a parent path is visited
/// before its children, and object keys / array indices in source order.
pub fn diff(a: &Value, b: &Value) -> Vec<Change> {
    let mut changes = Vec::new();
    diff_into(a, b, String::new(), &mut changes);
    changes
}

fn diff_into(a: &Value, b: &Value, path: String, out: &mut Vec<Change>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            // Keys present in `a`: removed or recurse.
            for (k, va) in ma {
                let child = format!("{}/{}", path, escape_pointer(k));
                match mb.get(k) {
                    Some(vb) => diff_into(va, vb, child, out),
                    None => out.push(Change::Removed {
                        path: child,
                        value: va.clone(),
                    }),
                }
            }
            // Keys present only in `b`: added.
            for (k, vb) in mb {
                if !ma.contains_key(k) {
                    let child = format!("{}/{}", path, escape_pointer(k));
                    out.push(Change::Added {
                        path: child,
                        value: vb.clone(),
                    });
                }
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            let common = aa.len().min(ab.len());
            for i in 0..common {
                let child = format!("{}/{}", path, i);
                diff_into(&aa[i], &ab[i], child, out);
            }
            // Extra elements in `a` were removed.
            for i in common..aa.len() {
                out.push(Change::Removed {
                    path: format!("{}/{}", path, i),
                    value: aa[i].clone(),
                });
            }
            // Extra elements in `b` were added.
            for i in common..ab.len() {
                out.push(Change::Added {
                    path: format!("{}/{}", path, i),
                    value: ab[i].clone(),
                });
            }
        }
        _ => {
            if a != b {
                out.push(Change::Changed {
                    path: if path.is_empty() {
                        "/".to_string()
                    } else {
                        path
                    },
                    from: a.clone(),
                    to: b.clone(),
                });
            }
        }
    }
}

/// Escape a key for use in a JSON Pointer (RFC 6901: `~` -> `~0`, `/` -> `~1`).
fn escape_pointer(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

/// Render changes as human-readable lines.
pub fn render_text(changes: &[Change]) -> String {
    if changes.is_empty() {
        return "(no differences)".to_string();
    }
    let mut lines = Vec::with_capacity(changes.len());
    for c in changes {
        let line = match c {
            Change::Added { path, value } => {
                format!("+ {}: {}", path, compact(value))
            }
            Change::Removed { path, value } => {
                format!("- {}: {}", path, compact(value))
            }
            Change::Changed { path, from, to } => {
                format!("~ {}: {} -> {}", path, compact(from), compact(to))
            }
        };
        lines.push(line);
    }
    lines.join("\n")
}

/// Render changes as a machine-readable JSON array of `{op, path, ...}`.
pub fn render_json(changes: &[Change]) -> Value {
    let arr: Vec<Value> = changes
        .iter()
        .map(|c| match c {
            Change::Added { path, value } => serde_json::json!({
                "op": "add", "path": path, "value": value
            }),
            Change::Removed { path, value } => serde_json::json!({
                "op": "remove", "path": path, "value": value
            }),
            Change::Changed { path, from, to } => serde_json::json!({
                "op": "change", "path": path, "from": from, "to": to
            }),
        })
        .collect();
    Value::Array(arr)
}

fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "<unserializable>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_add_remove_change() {
        let a: Value = serde_json::from_str(r#"{"keep":1,"drop":2,"mod":3}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"keep":1,"mod":4,"new":5}"#).unwrap();
        let changes = diff(&a, &b);

        assert!(changes.contains(&Change::Removed {
            path: "/drop".into(),
            value: Value::from(2),
        }));
        assert!(changes.contains(&Change::Changed {
            path: "/mod".into(),
            from: Value::from(3),
            to: Value::from(4),
        }));
        assert!(changes.contains(&Change::Added {
            path: "/new".into(),
            value: Value::from(5),
        }));
        // "keep" is unchanged -> no entry for it
        assert!(!changes.iter().any(|c| c.path() == "/keep"));
    }

    #[test]
    fn nested_and_array_changes() {
        let a: Value = serde_json::from_str(r#"{"o":{"x":1},"arr":[1,2,3]}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"o":{"x":2},"arr":[1,9]}"#).unwrap();
        let changes = diff(&a, &b);

        assert!(changes.contains(&Change::Changed {
            path: "/o/x".into(),
            from: Value::from(1),
            to: Value::from(2),
        }));
        assert!(changes.contains(&Change::Changed {
            path: "/arr/1".into(),
            from: Value::from(2),
            to: Value::from(9),
        }));
        // index 2 removed (b is shorter)
        assert!(changes.contains(&Change::Removed {
            path: "/arr/2".into(),
            value: Value::from(3),
        }));
    }

    #[test]
    fn identical_values_no_diff() {
        let a: Value = serde_json::from_str(r#"{"a":1,"b":[1,2]}"#).unwrap();
        assert!(diff(&a, &a).is_empty());
        assert_eq!(render_text(&diff(&a, &a)), "(no differences)");
    }

    #[test]
    fn json_output_shape() {
        let a: Value = serde_json::from_str(r#"{"x":1}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"x":2}"#).unwrap();
        let out = render_json(&diff(&a, &b));
        let expected: Value =
            serde_json::from_str(r#"[{"op":"change","path":"/x","from":1,"to":2}]"#).unwrap();
        assert_eq!(out, expected);
    }
}
