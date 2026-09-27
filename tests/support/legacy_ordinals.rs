//! Remove only ordinal fields to encode otherwise exact pre-schema-14 fixture payloads.

pub fn strip_ordinals(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.remove("ordinal");
            for child in object.values_mut() {
                strip_ordinals(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                strip_ordinals(child);
            }
        }
        _ => {}
    }
}
