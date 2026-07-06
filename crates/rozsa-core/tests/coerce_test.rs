use rozsa_core::coerce::coerce_arguments;
use serde_json::json;

#[test]
fn string_to_number() {
    let schema = json!({"type": "object", "properties": {"count": {"type": "number"}}});
    let args = json!({"count": "42"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"count": 42.0}));
}

#[test]
fn string_to_integer() {
    let schema = json!({"type": "object", "properties": {"line": {"type": "integer"}}});
    let args = json!({"line": "7"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"line": 7}));
}

#[test]
fn string_to_float() {
    let schema = json!({"type": "object", "properties": {"ratio": {"type": "number"}}});
    let args = json!({"ratio": "3.14"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"ratio": 3.14}));
}

#[test]
fn string_to_boolean() {
    let schema = json!({"type": "object", "properties": {"flag": {"type": "boolean"}}});
    let args = json!({"flag": "true"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"flag": true}));

    let args2 = json!({"flag": "false"});
    let result2 = coerce_arguments(&schema, args2);
    assert_eq!(result2, json!({"flag": false}));
}

#[test]
fn number_to_string() {
    let schema = json!({"type": "object", "properties": {"name": {"type": "string"}}});
    let args = json!({"name": 123});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"name": "123"}));
}

#[test]
fn bool_to_string() {
    let schema = json!({"type": "object", "properties": {"val": {"type": "string"}}});
    let args = json!({"val": true});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"val": "true"}));
}

#[test]
fn non_coercible_left_unchanged() {
    let schema = json!({"type": "object", "properties": {"count": {"type": "number"}}});
    let args = json!({"count": "abc"});
    let result = coerce_arguments(&schema, args.clone());
    assert_eq!(result, args);
}

#[test]
fn nested_object_coercion() {
    let schema = json!({
        "type": "object",
        "properties": {
            "config": {
                "type": "object",
                "properties": {
                    "timeout": {"type": "number"},
                    "verbose": {"type": "boolean"}
                }
            }
        }
    });
    let args = json!({"config": {"timeout": "30", "verbose": "true"}});
    let result = coerce_arguments(&schema, args);
    assert_eq!(
        result,
        json!({"config": {"timeout": 30.0, "verbose": true}})
    );
}

#[test]
fn array_items_coercion() {
    let schema = json!({
        "type": "object",
        "properties": {
            "ids": {"type": "array", "items": {"type": "integer"}}
        }
    });
    let args = json!({"ids": ["1", "2", "3"]});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"ids": [1, 2, 3]}));
}

#[test]
fn single_value_wrapped_to_array() {
    let schema = json!({
        "type": "object",
        "properties": {
            "tags": {"type": "array", "items": {"type": "string"}}
        }
    });
    let args = json!({"tags": "single"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"tags": ["single"]}));
}

#[test]
fn already_correct_type_unchanged() {
    let schema = json!({"type": "object", "properties": {"count": {"type": "number"}}});
    let args = json!({"count": 42});
    let result = coerce_arguments(&schema, args.clone());
    assert_eq!(result, args);
}

#[test]
fn unknown_properties_preserved() {
    let schema = json!({"type": "object", "properties": {"a": {"type": "number"}}});
    let args = json!({"a": "5", "b": "untouched"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"a": 5.0, "b": "untouched"}));
}

#[test]
fn null_args_passthrough() {
    let schema = json!({"type": "object", "properties": {"x": {"type": "number"}}});
    let args = json!({});
    let result = coerce_arguments(&schema, args.clone());
    assert_eq!(result, args);
}

#[test]
fn any_of_coercion() {
    let schema = json!({
        "type": "object",
        "properties": {
            "value": {
                "anyOf": [
                    {"type": "number"},
                    {"type": "string"}
                ]
            }
        }
    });
    let args = json!({"value": "42"});
    let result = coerce_arguments(&schema, args);
    assert_eq!(result, json!({"value": 42.0}));
}
