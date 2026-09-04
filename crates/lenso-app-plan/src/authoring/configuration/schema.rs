use serde_json::Value;

use super::{
    ConfigurationError, SCHEMA_METADATA_KEYWORDS, SUPPORTED_SCHEMA_KEYWORDS, invalid_configuration,
};

// Structural validation is separate from value matching so that errors in an
// inactive branch cannot be swallowed by `if`.
pub(super) fn check(
    schema: &Value,
    path: &str,
    instance: &str,
    depth: usize,
) -> Result<(), ConfigurationError> {
    let invalid = |detail: &str| invalid_configuration(instance, path, detail);
    if depth > 64 {
        return Err(invalid("configuration schema exceeds 64 levels"));
    }
    if schema.is_boolean() {
        return Ok(());
    }
    let object = schema
        .as_object()
        .ok_or_else(|| invalid("configuration schema must be an object or boolean"))?;
    for (key, value) in object {
        if !SUPPORTED_SCHEMA_KEYWORDS.contains(&key.as_str())
            && !SCHEMA_METADATA_KEYWORDS.contains(&key.as_str())
        {
            return Err(invalid_configuration(
                instance,
                path,
                format!("unsupported JSON Schema keyword `{key}`"),
            ));
        }
        let child_path = format!("{path}.{key}");
        match key.as_str() {
            "if" | "then" | "else" | "items" | "additionalProperties" => {
                check(value, &child_path, instance, depth + 1)?;
            }
            "allOf" => {
                let branches = value
                    .as_array()
                    .filter(|items| !items.is_empty())
                    .ok_or_else(|| invalid("schema allOf must be a non-empty array"))?;
                for (index, branch) in branches.iter().enumerate() {
                    check(
                        branch,
                        &format!("{child_path}[{index}]"),
                        instance,
                        depth + 1,
                    )?;
                }
            }
            "properties" => {
                let properties = value
                    .as_object()
                    .ok_or_else(|| invalid("schema properties must be an object"))?;
                for (name, property) in properties {
                    check(
                        property,
                        &format!("{child_path}.{name}"),
                        instance,
                        depth + 1,
                    )?;
                }
            }
            "required" => {
                if !value
                    .as_array()
                    .is_some_and(|names| names.iter().all(Value::is_string))
                {
                    return Err(invalid("schema required must be an array of strings"));
                }
            }
            "type" => {
                if !value.as_str().is_some_and(|kind| {
                    [
                        "array", "boolean", "integer", "null", "number", "object", "string",
                    ]
                    .contains(&kind)
                }) {
                    return Err(invalid("schema type must be a supported type string"));
                }
            }
            "enum" => {
                if value.as_array().is_none_or(Vec::is_empty) {
                    return Err(invalid("schema enum must be a non-empty array"));
                }
            }
            "minimum" | "maximum" => {
                if !value.is_number() {
                    return Err(invalid("schema numeric bound must be a number"));
                }
            }
            "minItems" | "maxItems" | "minLength" | "maxLength" => {
                if value.as_u64().is_none() {
                    return Err(invalid("schema size bound must be a non-negative integer"));
                }
            }
            "uniqueItems" | "x-lenso-sensitive" if !value.is_boolean() => {
                return Err(invalid("schema flag must be a boolean"));
            }
            _ => {}
        }
    }
    Ok(())
}
