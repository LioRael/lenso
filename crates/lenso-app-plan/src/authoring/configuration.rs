use serde_json::Value;

use super::DefinitionResolutionError;

const SUPPORTED_SCHEMA_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "const",
    "enum",
    "items",
    "minimum",
    "properties",
    "required",
    "type",
    "x-lenso-sensitive",
];
const SCHEMA_METADATA_KEYWORDS: &[&str] = &[
    "$anchor",
    "$comment",
    "$id",
    "$schema",
    "default",
    "deprecated",
    "description",
    "examples",
    "readOnly",
    "title",
    "writeOnly",
];

pub(super) fn resolve_configuration(
    defaults: &Value,
    app_values: &Value,
    schema: Option<&Value>,
    instance_key: &str,
) -> Result<Value, DefinitionResolutionError> {
    if !defaults.is_object() {
        return Err(invalid_configuration(
            instance_key,
            "$",
            "package configuration defaults must be an object",
        ));
    }
    let mut effective = defaults.clone();
    overlay_configuration(&mut effective, app_values);
    validate_configuration(&effective, schema, instance_key)?;
    Ok(effective)
}

fn overlay_configuration(base: &mut Value, overlay: &Value) {
    if let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            match base.get_mut(key) {
                Some(base_value) => overlay_configuration(base_value, value),
                None => {
                    base.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        base.clone_from(overlay);
    }
}

fn validate_configuration(
    configuration: &Value,
    schema: Option<&Value>,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    match schema {
        Some(schema) => validate_json_schema(configuration, schema, "$", instance_key),
        None if configuration
            .as_object()
            .is_some_and(serde_json::Map::is_empty) =>
        {
            Ok(())
        }
        None => Err(invalid_configuration(
            instance_key,
            "$",
            "non-empty configuration requires a package-owned schema",
        )),
    }
}

fn validate_json_schema(
    value: &Value,
    schema: &Value,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let schema = schema.as_object().ok_or_else(|| {
        invalid_configuration(
            instance_key,
            path,
            "configuration schema must be a JSON object",
        )
    })?;
    if let Some(keyword) = schema.keys().find(|keyword| {
        !SUPPORTED_SCHEMA_KEYWORDS.contains(&keyword.as_str())
            && !SCHEMA_METADATA_KEYWORDS.contains(&keyword.as_str())
    }) {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("unsupported JSON Schema keyword `{keyword}`"),
        ));
    }
    if schema
        .get("x-lenso-sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let secret_reference = value.as_object().is_some_and(|object| {
            object.len() == 1 && object.get("secret_ref").is_some_and(Value::is_string)
        });
        return secret_reference.then_some(()).ok_or_else(|| {
            invalid_configuration(instance_key, path, "sensitive value must be a secret_ref")
        });
    }
    validate_schema_type(value, schema, path, instance_key)?;
    validate_schema_numeric_constraints(value, schema, path, instance_key)?;
    if let Some(expected) = schema.get("const")
        && value != expected
    {
        return Err(invalid_configuration(
            instance_key,
            path,
            "value does not match schema const",
        ));
    }
    if let Some(values) = schema.get("enum") {
        let values = values.as_array().ok_or_else(|| {
            invalid_configuration(instance_key, path, "schema enum must be an array")
        })?;
        if !values.iter().any(|expected| expected == value) {
            return Err(invalid_configuration(
                instance_key,
                path,
                "value is not in schema enum",
            ));
        }
    }
    validate_required(value, schema, path, instance_key)?;
    validate_properties(value, schema, path, instance_key)?;
    validate_items(value, schema, path, instance_key)
}

fn validate_schema_numeric_constraints(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(minimum) = schema.get("minimum") else {
        return Ok(());
    };
    let minimum = minimum.as_f64().ok_or_else(|| {
        invalid_configuration(instance_key, path, "schema minimum must be a number")
    })?;
    let Some(value) = value.as_f64() else {
        return Ok(());
    };
    if value < minimum {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("number must be greater than or equal to {minimum}"),
        ));
    }
    Ok(())
}

fn validate_schema_type(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(expected) = schema.get("type") else {
        return Ok(());
    };
    let expected = expected
        .as_str()
        .ok_or_else(|| invalid_configuration(instance_key, path, "schema type must be a string"))?;
    if ![
        "array", "boolean", "integer", "null", "number", "object", "string",
    ]
    .contains(&expected)
    {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("unsupported JSON Schema type `{expected}`"),
        ));
    }
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let integer = value
        .as_number()
        .is_some_and(|number| number.is_i64() || number.is_u64());
    if expected != actual && !(expected == "integer" && integer) {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("expected {expected}, found {actual}"),
        ));
    }
    Ok(())
}

fn validate_required(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(required) = schema.get("required") else {
        return Ok(());
    };
    let required = required.as_array().ok_or_else(|| {
        invalid_configuration(instance_key, path, "schema required must be an array")
    })?;
    let object = value.as_object().ok_or_else(|| {
        invalid_configuration(instance_key, path, "required fields need an object")
    })?;
    for name in required {
        let name = name.as_str().ok_or_else(|| {
            invalid_configuration(
                instance_key,
                path,
                "schema required entries must be strings",
            )
        })?;
        if !object.contains_key(name) {
            return Err(invalid_configuration(
                instance_key,
                &format!("{path}.{name}"),
                "required field is missing",
            ));
        }
    }
    Ok(())
}

fn validate_properties(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let empty = serde_json::Map::new();
    let properties = match schema.get("properties") {
        Some(properties) => properties.as_object().ok_or_else(|| {
            invalid_configuration(instance_key, path, "schema properties must be an object")
        })?,
        None => &empty,
    };
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    for (name, child_schema) in properties {
        if let Some(child) = object.get(name) {
            validate_json_schema(child, child_schema, &format!("{path}.{name}"), instance_key)?;
        }
    }
    if let Some(additional) = schema.get("additionalProperties") {
        match additional {
            Value::Bool(true) => {}
            Value::Bool(false) => {
                if let Some(name) = object.keys().find(|name| !properties.contains_key(*name)) {
                    return Err(invalid_configuration(
                        instance_key,
                        &format!("{path}.{name}"),
                        "additional property is not allowed",
                    ));
                }
            }
            additional_schema => {
                for (name, child) in object {
                    if !properties.contains_key(name) {
                        validate_json_schema(
                            child,
                            additional_schema,
                            &format!("{path}.{name}"),
                            instance_key,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_items(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(item_schema) = schema.get("items") else {
        return Ok(());
    };
    if !item_schema.is_object() {
        return Err(invalid_configuration(
            instance_key,
            path,
            "schema items must be an object",
        ));
    }
    let Some(items) = value.as_array() else {
        return Ok(());
    };
    for (index, item) in items.iter().enumerate() {
        validate_json_schema(item, item_schema, &format!("{path}[{index}]"), instance_key)?;
    }
    Ok(())
}

fn invalid_configuration(
    instance_key: &str,
    path: &str,
    detail: impl Into<String>,
) -> DefinitionResolutionError {
    DefinitionResolutionError::InvalidConfiguration {
        instance_key: instance_key.to_owned(),
        detail: format!("{path}: {}", detail.into()),
    }
}
