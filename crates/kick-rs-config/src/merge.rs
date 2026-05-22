//! Deep-merge for `serde_json::Value` trees.
//!
//! Semantics chosen to match what configuration consumers typically
//! expect:
//!
//! - **Objects** are merged key-by-key recursively. Keys missing from
//!   the overlay keep their base value; keys present in both recurse.
//! - **Arrays** are *replaced* wholesale by the overlay. (We could
//!   support append/prepend strategies, but for config the consistent
//!   behavior is "this list replaces that list". Adopters needing
//!   array-merge can post-process the raw `Value`.)
//! - **Scalars** (and any non-object / non-null overlay) replace the
//!   base value.
//! - **`null`** in the overlay is treated as "do not override" — it
//!   leaves the base value alone. This lets TOML files use literal
//!   `key = null`-equivalents to fall through to defaults.

use serde_json::Value;

/// Deep-merge `overlay` into `base` in place. See module docs for
/// merge semantics.
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => deep_merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        // `null` overlay never overrides — leave base alone.
        (_, Value::Null) => {}
        (slot, v) => {
            *slot = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn objects_merge_recursively() {
        let mut base = json!({ "a": 1, "b": { "x": 1, "y": 1 } });
        let overlay = json!({ "b": { "y": 2, "z": 3 }, "c": 4 });
        deep_merge(&mut base, overlay);
        assert_eq!(
            base,
            json!({ "a": 1, "b": { "x": 1, "y": 2, "z": 3 }, "c": 4 })
        );
    }

    #[test]
    fn arrays_are_replaced_not_concatenated() {
        let mut base = json!({ "k": [1, 2, 3] });
        deep_merge(&mut base, json!({ "k": [9] }));
        assert_eq!(base, json!({ "k": [9] }));
    }

    #[test]
    fn null_overlay_does_not_override() {
        let mut base = json!({ "k": 42 });
        deep_merge(&mut base, json!({ "k": null }));
        assert_eq!(base, json!({ "k": 42 }));
    }

    #[test]
    fn scalar_overlay_replaces_object_at_same_key() {
        // Overlay type-changes a key: the new value wins.
        let mut base = json!({ "k": { "inner": 1 } });
        deep_merge(&mut base, json!({ "k": "now a string" }));
        assert_eq!(base, json!({ "k": "now a string" }));
    }
}
