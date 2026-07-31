use crate::{MODULE_MANIFEST_PROTOCOL, MODULE_RELEASE_PROTOCOL, ModuleManifest, ModuleRelease};
use schemars::JsonSchema;
use serde_json::{Value, json};

const MODULE_ID_PATTERN: &str = "^[a-z][a-z0-9_-]*/[a-z][a-z0-9_-]*$";
const SHA256_PATTERN: &str = "^sha256:[0-9a-f]{64}$";

pub fn module_manifest_schema() -> Value {
    generated_module_schema::<ModuleManifest>(MODULE_MANIFEST_PROTOCOL, "LensoModuleManifest")
}

pub fn module_release_schema() -> Value {
    generated_module_schema::<ModuleRelease>(MODULE_RELEASE_PROTOCOL, "LensoModuleRelease")
}

fn generated_module_schema<T: JsonSchema>(protocol: &str, title: &str) -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T))
        .expect("generated Module schema must serialize");
    let object = schema
        .as_object_mut()
        .expect("generated Module schema root must be an object");
    object.insert(
        "$id".to_owned(),
        Value::String(format!(
            "https://contracts.lenso.local/modules/{protocol}.schema.json"
        )),
    );
    object.insert("title".to_owned(), Value::String(title.to_owned()));
    tighten_module_schema(&mut schema, protocol);
    schema
}

fn tighten_module_schema(schema: &mut Value, protocol: &str) {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "protocol".to_owned(),
            json!({ "type": "string", "const": protocol }),
        );
        if let Some(module_id) = properties.get_mut("module_id") {
            *module_id = json!({ "type": "string", "pattern": MODULE_ID_PATTERN });
        }
        if let Some(manifest_digest) = properties.get_mut("manifest_digest") {
            *manifest_digest = json!({ "type": "string", "pattern": SHA256_PATTERN });
        }
    }
    if let Some(manifest) = schema
        .get_mut("$defs")
        .and_then(|defs| defs.get_mut("ModuleManifest"))
        .and_then(Value::as_object_mut)
        .and_then(|manifest| manifest.get_mut("properties"))
        .and_then(Value::as_object_mut)
    {
        manifest.insert(
            "protocol".to_owned(),
            json!({ "type": "string", "const": MODULE_MANIFEST_PROTOCOL }),
        );
        manifest.insert(
            "module_id".to_owned(),
            json!({ "type": "string", "pattern": MODULE_ID_PATTERN }),
        );
    }
}
