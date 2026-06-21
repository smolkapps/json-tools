//! Deep-merge of JSON values, later wins.
//!
//! Merge semantics:
//! - Two objects: merged key-by-key (recursively). Keys only in one side are
//!   kept. New keys from the right are appended in right-hand order, so
//!   existing key order is preserved (input order intact via `preserve_order`).
//! - Two arrays: governed by [`ArrayPolicy`] — `Replace` takes the right array
//!   wholesale, `Concat` appends the right's elements to the left's.
//! - Any other type mismatch (or scalars): the right value replaces the left.

use serde_json::Value;

/// How arrays are combined when both sides are arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrayPolicy {
    /// Right array replaces left array (default).
    #[default]
    Replace,
    /// Right array's elements are appended to the left array.
    Concat,
}

/// Deep-merge `b` into `a`, returning the merged value (`b` wins on conflict).
pub fn merge_two(a: &Value, b: &Value, policy: ArrayPolicy) -> Value {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            let mut out = ma.clone();
            for (k, vb) in mb {
                match out.get(k) {
                    Some(va) => {
                        let merged = merge_two(va, vb, policy);
                        out.insert(k.clone(), merged);
                    }
                    None => {
                        out.insert(k.clone(), vb.clone());
                    }
                }
            }
            Value::Object(out)
        }
        (Value::Array(aa), Value::Array(ab)) if policy == ArrayPolicy::Concat => {
            let mut out = aa.clone();
            out.extend(ab.iter().cloned());
            Value::Array(out)
        }
        // Replace policy for arrays, or any scalar / type-mismatch: b wins.
        (_, b) => b.clone(),
    }
}

/// Fold a non-empty slice of values left-to-right, later values winning.
///
/// Returns `Value::Null` if `values` is empty.
pub fn merge_all(values: &[Value], policy: ArrayPolicy) -> Value {
    let mut iter = values.iter();
    let mut acc = match iter.next() {
        Some(first) => first.clone(),
        None => return Value::Null,
    };
    for v in iter {
        acc = merge_two(&acc, v, policy);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_merge_later_wins() {
        let a: Value = serde_json::from_str(r#"{"a":1,"nested":{"x":1,"y":2}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":99,"nested":{"y":20,"z":30}}"#).unwrap();
        let out = merge_two(&a, &b, ArrayPolicy::Replace);
        let expected: Value =
            serde_json::from_str(r#"{"a":99,"nested":{"x":1,"y":20,"z":30}}"#).unwrap();
        assert_eq!(out, expected);
    }

    #[test]
    fn array_replace_policy() {
        let a: Value = serde_json::from_str(r#"{"list":[1,2,3]}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"list":[9]}"#).unwrap();
        let out = merge_two(&a, &b, ArrayPolicy::Replace);
        assert_eq!(out, serde_json::json!({"list":[9]}));
    }

    #[test]
    fn array_concat_policy() {
        let a: Value = serde_json::from_str(r#"{"list":[1,2,3]}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"list":[4,5]}"#).unwrap();
        let out = merge_two(&a, &b, ArrayPolicy::Concat);
        assert_eq!(out, serde_json::json!({"list":[1,2,3,4,5]}));
    }

    #[test]
    fn merge_all_three_later_wins() {
        let vals = vec![
            serde_json::json!({"v":1,"a":"first"}),
            serde_json::json!({"v":2,"b":"second"}),
            serde_json::json!({"v":3,"c":"third"}),
        ];
        let out = merge_all(&vals, ArrayPolicy::Replace);
        let expected = serde_json::json!({"v":3,"a":"first","b":"second","c":"third"});
        assert_eq!(out, expected);
    }

    #[test]
    fn merge_preserves_key_order() {
        // existing keys keep their order; new keys append in right order.
        let a: Value = serde_json::from_str(r#"{"zebra":1,"apple":2}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"apple":20,"mango":3}"#).unwrap();
        let out = merge_two(&a, &b, ArrayPolicy::Replace);
        let s = serde_json::to_string(&out).unwrap();
        // zebra first (from a), apple updated in place, mango appended last.
        assert_eq!(s, r#"{"zebra":1,"apple":20,"mango":3}"#);
    }

    #[test]
    fn scalar_replaced_by_object() {
        let a: Value = serde_json::from_str(r#"{"x":5}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"x":{"deep":true}}"#).unwrap();
        let out = merge_two(&a, &b, ArrayPolicy::Replace);
        assert_eq!(out, serde_json::json!({"x":{"deep":true}}));
    }

    #[test]
    fn merge_all_empty_is_null() {
        assert_eq!(merge_all(&[], ArrayPolicy::Replace), Value::Null);
    }
}
