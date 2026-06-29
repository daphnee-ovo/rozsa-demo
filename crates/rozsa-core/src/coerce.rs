use serde_json::Value;

/// Coerce tool arguments to match the types declared in the JSON schema.
/// Handles common LLM mistakes: string-encoded numbers/booleans, number/bool as string.
/// Non-coercible values are left unchanged — the tool's serde_json::from_value will catch them.
pub fn coerce_arguments(schema: &Value, args: Value) -> Value {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => coerce_object(schema, args),
        _ => args,
    }
}

fn coerce_object(schema: &Value, mut args: Value) -> Value {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return args;
    };
    let Some(obj) = args.as_object_mut() else {
        return args;
    };

    for (key, prop_schema) in properties {
        let Some(value) = obj.remove(key) else {
            continue;
        };
        let coerced = coerce_value(prop_schema, value);
        obj.insert(key.clone(), coerced);
    }

    args
}

fn coerce_value(schema: &Value, value: Value) -> Value {
    let Some(expected_type) = schema.get("type").and_then(Value::as_str) else {
        if let Some(variants) = schema.get("anyOf").or_else(|| schema.get("oneOf")) {
            if let Some(arr) = variants.as_array() {
                for variant in arr {
                    let coerced = coerce_value(variant, value.clone());
                    if coerced != value {
                        return coerced;
                    }
                }
            }
        }
        return value;
    };

    match expected_type {
        "number" | "integer" => coerce_to_number(expected_type, value),
        "boolean" => coerce_to_boolean(value),
        "string" => coerce_to_string(value),
        "array" => coerce_to_array(schema, value),
        "object" => coerce_object(schema, value),
        _ => value,
    }
}

fn coerce_to_number(expected: &str, value: Value) -> Value {
    match &value {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return value;
            }
            if expected == "integer" {
                if let Ok(n) = trimmed.parse::<i64>() {
                    return Value::Number(n.into());
                }
            }
            if let Ok(f) = trimmed.parse::<f64>() {
                if f.is_finite() {
                    if let Some(n) = serde_json::Number::from_f64(f) {
                        return Value::Number(n);
                    }
                }
            }
            value
        }
        Value::Bool(b) => Value::Number(if *b { 1 } else { 0 }.into()),
        Value::Null => Value::Number(0.into()),
        _ => value,
    }
}

fn coerce_to_boolean(value: Value) -> Value {
    match &value {
        Value::String(s) => match s.to_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => value,
        },
        Value::Number(n) => {
            if n.as_f64() == Some(1.0) {
                Value::Bool(true)
            } else if n.as_f64() == Some(0.0) {
                Value::Bool(false)
            } else {
                value
            }
        }
        Value::Null => Value::Bool(false),
        _ => value,
    }
}

fn coerce_to_string(value: Value) -> Value {
    match &value {
        Value::Number(n) => Value::String(n.to_string()),
        Value::Bool(b) => Value::String(b.to_string()),
        Value::Null => Value::String(String::new()),
        _ => value,
    }
}

fn coerce_to_array(schema: &Value, value: Value) -> Value {
    if value.is_array() {
        if let Some(items_schema) = schema.get("items") {
            if let Value::Array(arr) = value {
                let coerced: Vec<Value> = arr
                    .into_iter()
                    .map(|item| coerce_value(items_schema, item))
                    .collect();
                return Value::Array(coerced);
            }
        }
        return value;
    }
    if let Some(items_schema) = schema.get("items") {
        Value::Array(vec![coerce_value(items_schema, value)])
    } else {
        Value::Array(vec![value])
    }
}
