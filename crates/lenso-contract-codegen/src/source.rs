//! Deterministic locked artifacts derived from compiled Rust Capability source.

use std::{collections::BTreeMap, fmt::Write as _, path::Path};

use lenso_contract_authoring::CapabilitySnapshot;
use serde_json::{Map, Value, json};

use crate::{CodegenError, check_artifact, load_descriptor, write_artifact};

/// Writes one Descriptor and its package-local Schemas from compiled source types.
pub fn write_source_snapshot(
    snapshot: &CapabilitySnapshot,
    descriptor_path: &Path,
) -> Result<(), CodegenError> {
    for (path, source) in snapshot_artifacts(snapshot, descriptor_path)? {
        write_artifact(&path, &source)?;
    }
    load_descriptor(descriptor_path).map(|_| ())
}

/// Fails unless every committed Descriptor and Schema byte matches compiled source.
pub fn check_source_snapshot(
    snapshot: &CapabilitySnapshot,
    descriptor_path: &Path,
) -> Result<(), CodegenError> {
    for (path, source) in snapshot_artifacts(snapshot, descriptor_path)? {
        check_artifact(&path, &source)?;
    }
    load_descriptor(descriptor_path).map(|_| ())
}

fn snapshot_artifacts(
    snapshot: &CapabilitySnapshot,
    descriptor_path: &Path,
) -> Result<BTreeMap<std::path::PathBuf, String>, CodegenError> {
    if snapshot.operations.is_empty() {
        return Err(CodegenError::InvalidDescriptor {
            detail: "a source Capability must declare at least one Operation".to_owned(),
        });
    }
    let root = descriptor_path.parent().unwrap_or_else(|| Path::new("."));
    let mut artifacts = BTreeMap::new();
    let mut operations = Vec::with_capacity(snapshot.operations.len());
    for operation in &snapshot.operations {
        let stem = schema_stem(&operation.name)?;
        let (request_suffix, response_suffix) = match operation.interaction.as_str() {
            "request" => ("request", "response"),
            "stream" => ("open", "message"),
            interaction => {
                return Err(CodegenError::InvalidDescriptor {
                    detail: format!(
                        "source Operation `{}` has unsupported interaction `{interaction}`",
                        operation.name
                    ),
                });
            }
        };
        let request = format!("schemas/{stem}-{request_suffix}.schema.json");
        let response = format!("schemas/{stem}-{response_suffix}.schema.json");
        let domain_error = format!("schemas/{stem}-error.schema.json");
        operations.push(json!({
            "name": operation.name,
            "interaction": operation.interaction,
            "request_schema": request,
            "response_schema": response,
            "domain_error_schema": domain_error,
        }));
        artifacts.insert(root.join(&request), pretty_json(&operation.request_schema)?);
        artifacts.insert(
            root.join(&response),
            pretty_json(&operation.response_schema)?,
        );
        artifacts.insert(
            root.join(&domain_error),
            pretty_json(&operation.domain_error_schema)?,
        );
    }
    let descriptor = Value::Object(Map::from_iter([
        (
            "id".to_owned(),
            Value::String(snapshot.capability_id.clone()),
        ),
        (
            "version".to_owned(),
            Value::String(snapshot.version.clone()),
        ),
        ("portable".to_owned(), Value::Bool(snapshot.portable)),
        (
            "cross_lane_transfer".to_owned(),
            Value::Bool(snapshot.cross_lane_transfer),
        ),
        ("operations".to_owned(), Value::Array(operations)),
    ]));
    artifacts.insert(descriptor_path.to_path_buf(), pretty_json(&descriptor)?);
    Ok(artifacts)
}

fn schema_stem(operation: &str) -> Result<String, CodegenError> {
    if operation.is_empty()
        || !operation
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(CodegenError::InvalidDescriptor {
            detail: format!("source Operation `{operation}` cannot form a Schema filename"),
        });
    }
    Ok(operation.replace('_', "-"))
}

fn pretty_json(value: &Value) -> Result<String, CodegenError> {
    let mut source = String::new();
    render_value(value, &mut source, 0, RenderContext::Root)?;
    source.push('\n');
    Ok(source)
}

#[derive(Clone, Copy)]
enum RenderContext {
    Root,
    Descriptor,
    Operation,
    Schema,
    Properties,
    RequiredArray,
}

fn render_value(
    value: &Value,
    output: &mut String,
    indent: usize,
    context: RenderContext,
) -> Result<(), CodegenError> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            output.push_str(&serialize_scalar(value)?);
        }
        Value::Array(values) => render_array(values, output, indent, context)?,
        Value::Object(object) => render_object(object, output, indent, context)?,
    }
    Ok(())
}

fn render_array(
    values: &[Value],
    output: &mut String,
    indent: usize,
    context: RenderContext,
) -> Result<(), CodegenError> {
    if values.is_empty() {
        output.push_str("[]");
        return Ok(());
    }
    if values.iter().all(Value::is_string)
        && !(matches!(context, RenderContext::RequiredArray) && values.len() > 5)
    {
        output.push('[');
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(&serialize_scalar(value)?);
        }
        output.push(']');
        return Ok(());
    }
    output.push_str("[\n");
    for (index, value) in values.iter().enumerate() {
        push_indent(output, indent + 1);
        let item_context = match context {
            RenderContext::Descriptor => RenderContext::Operation,
            _ => RenderContext::Schema,
        };
        render_value(value, output, indent + 1, item_context)?;
        if index + 1 != values.len() {
            output.push(',');
        }
        output.push('\n');
    }
    push_indent(output, indent);
    output.push(']');
    Ok(())
}

fn render_object(
    object: &Map<String, Value>,
    output: &mut String,
    indent: usize,
    context: RenderContext,
) -> Result<(), CodegenError> {
    if object.is_empty() {
        output.push_str("{}");
        return Ok(());
    }
    if object.len() == 1 && object.contains_key("const") {
        output.push_str("{ \"const\": ");
        render_value(&object["const"], output, indent, RenderContext::Schema)?;
        output.push_str(" }");
        return Ok(());
    }

    let object_context = match context {
        RenderContext::Root if object.contains_key("id") && object.contains_key("operations") => {
            RenderContext::Descriptor
        }
        RenderContext::Root => RenderContext::Schema,
        other => other,
    };
    let keys = ordered_keys(object, object_context);
    output.push_str("{\n");
    for (index, key) in keys.iter().enumerate() {
        push_indent(output, indent + 1);
        output.push_str(&serialize_scalar(&Value::String((*key).to_owned()))?);
        output.push_str(": ");
        let child_context = if *key == "operations" {
            RenderContext::Descriptor
        } else if *key == "required" {
            RenderContext::RequiredArray
        } else {
            RenderContext::Schema
        };
        if *key == "properties" {
            render_properties(&object[*key], output, indent + 1)?;
        } else {
            render_value(&object[*key], output, indent + 1, child_context)?;
        }
        if index + 1 != keys.len() {
            output.push(',');
        }
        output.push('\n');
    }
    push_indent(output, indent);
    output.push('}');
    Ok(())
}

fn ordered_keys(object: &Map<String, Value>, context: RenderContext) -> Vec<&str> {
    let preferred: &[&str] = match context {
        RenderContext::Descriptor => &[
            "id",
            "version",
            "portable",
            "cross_lane_transfer",
            "operations",
        ],
        RenderContext::Operation => &[
            "name",
            "interaction",
            "request_schema",
            "response_schema",
            "domain_error_schema",
        ],
        RenderContext::Schema | RenderContext::Root | RenderContext::RequiredArray => &[
            "$schema",
            "type",
            "format",
            "minimum",
            "maximum",
            "required",
            "properties",
            "propertyNames",
            "additionalProperties",
            "minLength",
            "maxLength",
            "enum",
            "maxItems",
            "items",
            "oneOf",
            "const",
        ],
        RenderContext::Properties => return ordered_property_keys(object),
    };
    let mut keys = Vec::with_capacity(object.len());
    for key in preferred {
        if object.contains_key(*key) {
            keys.push(*key);
        }
    }
    let mut remaining: Vec<_> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !keys.contains(key))
        .collect();
    remaining.sort_unstable();
    keys.extend(remaining);
    keys
}

fn ordered_property_keys(object: &Map<String, Value>) -> Vec<&str> {
    object.keys().map(String::as_str).collect()
}

fn render_properties(
    value: &Value,
    output: &mut String,
    indent: usize,
) -> Result<(), CodegenError> {
    let Some(properties) = value.as_object() else {
        return render_value(value, output, indent, RenderContext::Properties);
    };
    if properties.is_empty() {
        output.push_str("{}");
        return Ok(());
    }
    let keys: Vec<_> = properties.keys().map(String::as_str).collect();

    output.push_str("{\n");
    for (index, key) in keys.iter().enumerate() {
        push_indent(output, indent + 1);
        output.push_str(&serialize_scalar(&Value::String((*key).to_owned()))?);
        output.push_str(": ");
        render_value(&properties[*key], output, indent + 1, RenderContext::Schema)?;
        if index + 1 != keys.len() {
            output.push(',');
        }
        output.push('\n');
    }
    push_indent(output, indent);
    output.push('}');
    Ok(())
}

fn serialize_scalar(value: &Value) -> Result<String, CodegenError> {
    serde_json::to_string(value).map_err(|error| CodegenError::InvalidDescriptor {
        detail: format!("derived snapshot could not serialize: {error}"),
    })
}

fn push_indent(output: &mut String, indent: usize) {
    let _ = write!(output, "{:width$}", "", width = indent * 2);
}
