use serde_json::Value;

mod schema;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConfigurationError {
    pub(super) detail: String,
}

const SUPPORTED_SCHEMA_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "allOf",
    "if",
    "then",
    "else",
    "const",
    "enum",
    "items",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "uniqueItems",
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

pub(super) fn resolve_configuration_layers(
    defaults: &Value,
    overlays: &[&Value],
    schema: Option<&Value>,
    instance_key: &str,
) -> Result<Value, ConfigurationError> {
    if !defaults.is_object() {
        return Err(invalid_configuration(
            instance_key,
            "$",
            "package configuration defaults must be an object",
        ));
    }
    let mut effective = defaults.clone();
    for overlay in overlays {
        overlay_configuration(&mut effective, overlay);
    }
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
) -> Result<(), ConfigurationError> {
    match schema {
        Some(schema) => {
            // Check every branch before matching: a malformed condition must not
            // masquerade as a false condition and select a permissive alternative.
            schema::check(schema, "$", instance_key, 0)?;
            validate_json_schema(configuration, schema, "$", instance_key)
        }
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
) -> Result<(), ConfigurationError> {
    if let Some(allowed) = schema.as_bool() {
        return if allowed {
            Ok(())
        } else {
            Err(invalid_configuration(
                instance_key,
                path,
                "value is forbidden by schema",
            ))
        };
    }
    let schema = schema.as_object().ok_or_else(|| {
        invalid_configuration(
            instance_key,
            path,
            "configuration schema must be a JSON object",
        )
    })?;
    validate_combinators(value, schema, path, instance_key)?;
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
    validate_collection_constraints(value, schema, path, instance_key)?;
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

fn validate_combinators(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), ConfigurationError> {
    if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
        for branch in branches {
            validate_json_schema(value, branch, path, instance_key)?;
        }
    }
    if let Some(condition) = schema.get("if") {
        let selected = if validate_json_schema(value, condition, path, instance_key).is_ok() {
            "then"
        } else {
            "else"
        };
        if let Some(branch) = schema.get(selected) {
            validate_json_schema(value, branch, path, instance_key)?;
        }
    }
    Ok(())
}

fn validate_collection_constraints(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), ConfigurationError> {
    let size = match value {
        Value::String(text) => Some((text.chars().count(), "minLength", "maxLength")),
        Value::Array(items) => Some((items.len(), "minItems", "maxItems")),
        _ => None,
    };
    if let Some((length, minimum, maximum)) = size {
        for (keyword, too_small) in [(minimum, true), (maximum, false)] {
            if let Some(limit) = schema.get(keyword).and_then(Value::as_u64)
                && if too_small {
                    (length as u64) < limit
                } else {
                    (length as u64) > limit
                }
            {
                return Err(invalid_configuration(
                    instance_key,
                    path,
                    format!("value violates {keyword}"),
                ));
            }
        }
    }
    if schema.get("uniqueItems") == Some(&Value::Bool(true))
        && let Some(items) = value.as_array()
        && items
            .iter()
            .enumerate()
            .any(|(index, item)| items[..index].contains(item))
    {
        return Err(invalid_configuration(
            instance_key,
            path,
            "array items must be unique",
        ));
    }
    Ok(())
}

fn validate_schema_numeric_constraints(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), ConfigurationError> {
    let Some(value) = value.as_f64() else {
        return Ok(());
    };
    for (keyword, lower) in [("minimum", true), ("maximum", false)] {
        if let Some(limit) = schema.get(keyword).and_then(Value::as_f64)
            && if lower { value < limit } else { value > limit }
        {
            return Err(invalid_configuration(
                instance_key,
                path,
                format!("number violates {keyword}"),
            ));
        }
    }
    Ok(())
}

fn validate_schema_type(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), ConfigurationError> {
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
) -> Result<(), ConfigurationError> {
    let Some(required) = schema.get("required") else {
        return Ok(());
    };
    let required = required.as_array().ok_or_else(|| {
        invalid_configuration(instance_key, path, "schema required must be an array")
    })?;
    let Some(object) = value.as_object() else {
        return Ok(());
    };
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
) -> Result<(), ConfigurationError> {
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
) -> Result<(), ConfigurationError> {
    let Some(item_schema) = schema.get("items") else {
        return Ok(());
    };
    if !item_schema.is_object() && !item_schema.is_boolean() {
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
    _instance_key: &str,
    path: &str,
    detail: impl Into<String>,
) -> ConfigurationError {
    ConfigurationError {
        detail: format!("{path}: {}", detail.into()),
    }
}
