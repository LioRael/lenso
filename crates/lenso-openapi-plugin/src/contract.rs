//! Validation of the private type-derived strict `OpenAPI` contract fragment.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::OpenApiConfig;

/// Validates the type-derived fragment carried by an Endpoint description.
///
/// The fragment uses normal `OpenAPI` representation for parameters, request
/// bodies, and responses. That lets the validator compare the exact surface a
/// client generator consumes instead of introducing a parallel DTO schema.
pub(crate) fn validate(
    config: &OpenApiConfig,
    route_id: &str,
    method: &str,
    path: &str,
    operation: &Map<String, Value>,
    contract: &Value,
) -> Result<(), String> {
    let contract = contract.as_object().ok_or_else(|| {
        format!("route {route_id} has a non-object generated strict OpenAPI contract")
    })?;
    require_string(contract, "route_id", route_id, route_id)?;
    require_string(contract, "method", method, route_id)?;
    require_string(contract, "path", path, route_id)?;
    let expected = contract
        .get("operation")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("route {route_id} has a generated contract without an operation"))?;

    validate_parameters(config, route_id, expected, operation)?;
    validate_request_body(config, route_id, expected, operation)?;
    validate_responses(config, route_id, expected, operation)?;
    Ok(())
}

fn require_string(
    contract: &Map<String, Value>,
    field: &str,
    actual: &str,
    route_id: &str,
) -> Result<(), String> {
    let expected = contract.get(field).and_then(Value::as_str).ok_or_else(|| {
        format!("route {route_id} has a generated contract without string {field}")
    })?;
    if expected != actual {
        return Err(format!(
            "route {route_id} strict OpenAPI contract drifted {field}: generated `{expected}`, declared `{actual}`"
        ));
    }
    Ok(())
}

fn validate_parameters(
    config: &OpenApiConfig,
    route_id: &str,
    expected: &Map<String, Value>,
    operation: &Map<String, Value>,
) -> Result<(), String> {
    let expected = parameter_map(config, route_id, expected.get("parameters"), "generated")?;
    let actual = parameter_map(config, route_id, operation.get("parameters"), "authored")?;
    if expected.keys().collect::<Vec<_>>() != actual.keys().collect::<Vec<_>>() {
        return Err(format!(
            "route {route_id} strict OpenAPI path or query parameters differ from its typed handler"
        ));
    }
    for (key, expected) in expected {
        let actual = actual
            .get(&key)
            .expect("parameter map keys were compared before lookup");
        if expected != *actual {
            return Err(format!(
                "route {route_id} strict OpenAPI {} parameter `{}` differs from its typed handler",
                key.0, key.1
            ));
        }
    }
    Ok(())
}

fn parameter_map(
    config: &OpenApiConfig,
    route_id: &str,
    value: Option<&Value>,
    origin: &str,
) -> Result<BTreeMap<(String, String), Parameter>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("route {route_id} has non-array {origin} OpenAPI parameters"))?;
    let mut parameters = BTreeMap::new();
    for value in values {
        let value = resolve_component(config, value, "parameters", route_id)?;
        let object = value.as_object().ok_or_else(|| {
            format!("route {route_id} has a non-object {origin} OpenAPI parameter")
        })?;
        let Some(location) = object.get("in").and_then(Value::as_str) else {
            return Err(format!(
                "route {route_id} has an OpenAPI parameter without a location"
            ));
        };
        if !matches!(location, "path" | "query") {
            continue;
        }
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("route {route_id} has a {location} parameter without a name"))?;
        let required = object
            .get("required")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                format!("route {route_id} {location} parameter {name} must declare required")
            })?;
        if location == "path" && !required {
            return Err(format!(
                "route {route_id} path parameter {name} must be required"
            ));
        }
        validate_parameter_serialization(object, location, route_id, name)?;
        let schema = object
            .get("schema")
            .ok_or_else(|| format!("route {route_id} {location} parameter {name} has no schema"))?;
        let key = (location.to_owned(), name.to_owned());
        if parameters
            .insert(
                key,
                Parameter {
                    required,
                    schema: canonical_schema(config, schema, route_id)?,
                },
            )
            .is_some()
        {
            return Err(format!(
                "route {route_id} declares the same {location} parameter {name} more than once"
            ));
        }
    }
    Ok(parameters)
}

#[derive(Debug, Eq, PartialEq)]
struct Parameter {
    required: bool,
    schema: Value,
}

fn validate_parameter_serialization(
    parameter: &Map<String, Value>,
    location: &str,
    route_id: &str,
    name: &str,
) -> Result<(), String> {
    let default_style = if location == "path" { "simple" } else { "form" };
    if parameter
        .get("style")
        .and_then(Value::as_str)
        .is_some_and(|style| style != default_style)
    {
        return Err(format!(
            "route {route_id} strict OpenAPI {location} parameter {name} uses a non-default serialization style"
        ));
    }
    let default_explode = location == "query";
    if parameter
        .get("explode")
        .and_then(Value::as_bool)
        .is_some_and(|explode| explode != default_explode)
    {
        return Err(format!(
            "route {route_id} strict OpenAPI {location} parameter {name} uses a non-default explode setting"
        ));
    }
    if parameter
        .get("allowReserved")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!(
            "route {route_id} strict OpenAPI {location} parameter {name} sets allowReserved"
        ));
    }
    Ok(())
}

fn validate_request_body(
    config: &OpenApiConfig,
    route_id: &str,
    expected: &Map<String, Value>,
    operation: &Map<String, Value>,
) -> Result<(), String> {
    let expected = expected.get("requestBody");
    let actual = operation.get("requestBody");
    match (expected, actual) {
        (None, None) => Ok(()),
        (None, Some(_)) | (Some(_), None) => Err(format!(
            "route {route_id} strict OpenAPI request body differs from its typed handler"
        )),
        (Some(expected), Some(actual)) => {
            let expected = resolve_component(config, expected, "requestBodies", route_id)?;
            let actual = resolve_component(config, actual, "requestBodies", route_id)?;
            let expected = request_body(config, route_id, &expected, "generated")?;
            let actual = request_body(config, route_id, &actual, "authored")?;
            if expected != actual {
                return Err(format!(
                    "route {route_id} strict OpenAPI JSON request body differs from its typed handler"
                ));
            }
            Ok(())
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct RequestBody {
    required: bool,
    schema: Value,
}

fn request_body(
    config: &OpenApiConfig,
    route_id: &str,
    value: &Value,
    origin: &str,
) -> Result<RequestBody, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("route {route_id} has a non-object {origin} OpenAPI requestBody"))?;
    let required = object
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let schema = media_schema(config, route_id, object, "application/json", origin)?;
    Ok(RequestBody { required, schema })
}

fn validate_responses(
    config: &OpenApiConfig,
    route_id: &str,
    expected: &Map<String, Value>,
    operation: &Map<String, Value>,
) -> Result<(), String> {
    let expected = response_map(config, route_id, expected.get("responses"), "generated")?;
    let actual = response_map(config, route_id, operation.get("responses"), "authored")?;
    if expected.keys().collect::<Vec<_>>() != actual.keys().collect::<Vec<_>>() {
        return Err(format!(
            "route {route_id} strict OpenAPI success or known error responses differ from its typed handler"
        ));
    }
    for (status, expected) in expected {
        let actual = actual
            .get(&status)
            .expect("response map keys were compared before lookup");
        if expected != *actual {
            return Err(format!(
                "route {route_id} strict OpenAPI response {status} differs from its typed handler"
            ));
        }
    }
    Ok(())
}

fn response_map(
    config: &OpenApiConfig,
    route_id: &str,
    value: Option<&Value>,
    origin: &str,
) -> Result<BTreeMap<String, Response>, String> {
    let object = value.and_then(Value::as_object).ok_or_else(|| {
        format!("route {route_id} has a non-object {origin} OpenAPI responses field")
    })?;
    let mut responses = BTreeMap::new();
    for (status, value) in object {
        if !is_status_code(status) {
            return Err(format!(
                "route {route_id} strict OpenAPI response key {status} is not an explicit HTTP status"
            ));
        }
        let value = resolve_component(config, value, "responses", route_id)?;
        let object = value.as_object().ok_or_else(|| {
            format!("route {route_id} has a non-object {origin} response {status}")
        })?;
        let media_type = if status.starts_with('2') {
            "application/json"
        } else {
            "application/problem+json"
        };
        let schema = media_schema(config, route_id, object, media_type, origin)?;
        responses.insert(status.clone(), Response { media_type, schema });
    }
    Ok(responses)
}

#[derive(Debug, Eq, PartialEq)]
struct Response {
    media_type: &'static str,
    schema: Value,
}

fn is_status_code(status: &str) -> bool {
    status.len() == 3
        && status
            .parse::<u16>()
            .is_ok_and(|status| (100..=599).contains(&status))
}

fn media_schema(
    config: &OpenApiConfig,
    route_id: &str,
    object: &Map<String, Value>,
    expected_media_type: &str,
    origin: &str,
) -> Result<Value, String> {
    let content = object
        .get("content")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("route {route_id} {origin} OpenAPI content must be an object"))?;
    if content.len() != 1 || !content.contains_key(expected_media_type) {
        return Err(format!(
            "route {route_id} {origin} OpenAPI content must contain only {expected_media_type}"
        ));
    }
    let media = content
        .get(expected_media_type)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!(
                "route {route_id} {origin} OpenAPI {expected_media_type} content must be an object"
            )
        })?;
    let schema = media.get("schema").ok_or_else(|| {
        format!("route {route_id} {origin} OpenAPI {expected_media_type} content has no schema")
    })?;
    canonical_schema(config, schema, route_id)
}

fn resolve_component(
    config: &OpenApiConfig,
    value: &Value,
    section: &str,
    route_id: &str,
) -> Result<Value, String> {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return Ok(value.clone());
    };
    let prefix = format!("#/components/{section}/");
    let Some(name) = reference.strip_prefix(&prefix) else {
        return Err(format!(
            "route {route_id} uses unsupported OpenAPI reference {reference}"
        ));
    };
    if name.is_empty() || name.contains('/') {
        return Err(format!(
            "route {route_id} uses invalid OpenAPI reference {reference}"
        ));
    }
    config
        .components()
        .and_then(|components| components.get(section))
        .and_then(Value::as_object)
        .and_then(|components| components.get(name))
        .cloned()
        .ok_or_else(|| format!("route {route_id} references missing OpenAPI component {reference}"))
}

fn canonical_schema(
    config: &OpenApiConfig,
    schema: &Value,
    route_id: &str,
) -> Result<Value, String> {
    canonical_schema_with_root(config, schema, schema, route_id, &mut BTreeSet::new())
}

fn canonical_schema_with_root(
    config: &OpenApiConfig,
    schema: &Value,
    local_root: &Value,
    route_id: &str,
    references: &mut BTreeSet<String>,
) -> Result<Value, String> {
    match schema {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let (target, target_root, key) =
                    resolve_schema_reference(config, reference, local_root, route_id)?;
                if !references.insert(key.clone()) {
                    return Err(format!(
                        "route {route_id} has a cyclic OpenAPI schema reference {reference}"
                    ));
                }
                let mut normalized = canonical_schema_with_root(
                    config,
                    &target,
                    &target_root,
                    route_id,
                    references,
                )?;
                references.remove(&key);
                let Value::Object(normalized_object) = &mut normalized else {
                    return Err(format!(
                        "route {route_id} OpenAPI schema reference {reference} did not resolve to an object"
                    ));
                };
                for (key, value) in object {
                    if key == "$ref" || ignored_schema_keyword(key) {
                        continue;
                    }
                    normalized_object.insert(
                        key.clone(),
                        canonical_schema_with_root(
                            config, value, local_root, route_id, references,
                        )?,
                    );
                }
                return Ok(normalized);
            }

            let mut normalized = Map::new();
            for (key, value) in object {
                if ignored_schema_keyword(key) || key == "$defs" {
                    continue;
                }
                let mut value =
                    canonical_schema_with_root(config, value, local_root, route_id, references)?;
                if matches!(key.as_str(), "required" | "enum") {
                    sort_json_array(&mut value);
                }
                normalized.insert(key.clone(), value);
            }
            Ok(Value::Object(normalized))
        }
        Value::Array(values) => values
            .iter()
            .map(|value| {
                canonical_schema_with_root(config, value, local_root, route_id, references)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Ok(schema.clone()),
    }
}

fn resolve_schema_reference(
    config: &OpenApiConfig,
    reference: &str,
    local_root: &Value,
    route_id: &str,
) -> Result<(Value, Value, String), String> {
    if let Some(name) = reference.strip_prefix("#/$defs/") {
        let target = local_root
            .get("$defs")
            .and_then(Value::as_object)
            .and_then(|definitions| definitions.get(name))
            .cloned()
            .ok_or_else(|| {
                format!("route {route_id} references missing local OpenAPI definition {reference}")
            })?;
        return Ok((target, local_root.clone(), format!("local:{reference}")));
    }
    let Some(name) = reference.strip_prefix("#/components/schemas/") else {
        return Err(format!(
            "route {route_id} uses unsupported OpenAPI schema reference {reference}"
        ));
    };
    if name.is_empty() || name.contains('/') {
        return Err(format!(
            "route {route_id} uses invalid OpenAPI schema reference {reference}"
        ));
    }
    let target = config
        .components()
        .and_then(|components| components.get("schemas"))
        .and_then(Value::as_object)
        .and_then(|schemas| schemas.get(name))
        .cloned()
        .ok_or_else(|| {
            format!("route {route_id} references missing OpenAPI schema component {reference}")
        })?;
    Ok((target.clone(), target, format!("component:{reference}")))
}

fn ignored_schema_keyword(key: &str) -> bool {
    matches!(
        key,
        "$schema"
            | "$id"
            | "title"
            | "description"
            | "examples"
            | "example"
            | "default"
            | "deprecated"
            | "readOnly"
            | "writeOnly"
    )
}

fn sort_json_array(value: &mut Value) {
    let Some(values) = value.as_array_mut() else {
        return;
    };
    values.sort_by_key(ToString::to_string);
}
