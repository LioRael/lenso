use crate::{
    ArtifactReference, CONSOLE_MODULE_PROTOCOL, CONSOLE_MODULE_PROTOCOL_MAJOR,
    CONSOLE_UI_ESM_FORMAT, ConsoleSurface, ConsoleSurfacePresentation, ConsoleUiArtifact,
    ConsoleUiArtifactEntry, ConsoleUiArtifactFormat, ConsoleUiArtifactStyleAsset,
    LinkedModuleDelivery, MODULE_MANIFEST_PROTOCOL, MODULE_RELEASE_PROTOCOL,
    ModuleConfigActivation, ModuleConfigContract, ModuleConfigField, ModuleConfigFieldType,
    ModuleConfigMutability, ModuleConfigScope, ModuleDelivery, ModuleManifest, ModuleRelease,
};
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

pub fn console_module_manifest_schema() -> Value {
    let mut schema = generated_console_schema::<crate::ConsoleModuleManifest>(
        "lenso.console-module.v1.schema.json",
        "LensoConsoleModuleManifest",
    );
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "protocol".to_owned(),
            json!({ "type": "string", "const": CONSOLE_MODULE_PROTOCOL }),
        );
        properties.insert(
            "surfaces".to_owned(),
            json!({ "type": "array", "minItems": 1, "items": { "$ref": "#/$defs/ConsoleModuleSurface" } }),
        );
    }
    schema
}

pub fn console_ui_artifact_schema() -> Value {
    let mut schema = generated_console_schema::<ConsoleUiArtifact>(
        "lenso.console-ui-esm.v1.schema.json",
        "LensoConsoleUiEsmArtifact",
    );
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "format".to_owned(),
            json!({ "type": "string", "const": CONSOLE_UI_ESM_FORMAT }),
        );
        properties.insert(
            "protocolMajor".to_owned(),
            json!({ "type": "integer", "const": CONSOLE_MODULE_PROTOCOL_MAJOR }),
        );
    }
    schema
}

/// Deterministic positive and negative vectors shared by framework and
/// Console loaders. Negative cases are deliberately represented as complete
/// Module Release documents so each consumer can run its own validator.
pub fn console_contract_vectors() -> Value {
    let positive = sample_console_release();
    let valid = serde_json::to_value(&positive).expect("Console vector release serializes");
    let negative = vec![
        negative_release("protocol", "protocol", &valid, |release| {
            release["console_ui_artifact"]["manifest"]["protocol"] =
                Value::String("lenso.console-module.v9".to_owned());
        }),
        negative_release("host-api-range", "compatibility", &valid, |release| {
            release["compatibility"]["host_api_requirement"] = Value::String("^9.0.0".to_owned());
        }),
        negative_release("surface-path", "path", &valid, |release| {
            release["console_ui_artifact"]["manifest"]["surfaces"][0]["path"] =
                Value::String("/data/../escape".to_owned());
        }),
        negative_release("entry", "entry", &valid, |release| {
            release["console_ui_artifact"]["entry"] = Value::String("missing.js".to_owned());
        }),
        negative_release("style-asset-path", "style_asset", &valid, |release| {
            release["console_ui_artifact"]["styleAssets"][0]["path"] =
                Value::String("../style.css".to_owned());
        }),
        negative_release("style-asset-entry", "style_asset", &valid, |release| {
            release["console_ui_artifact"]["styleAssets"][0]["path"] =
                Value::String("assets/missing.css".to_owned());
        }),
        negative_release("module-identity", "identity", &valid, |release| {
            release["console_ui_artifact"]["manifest"]["moduleId"] =
                Value::String("other/module".to_owned());
        }),
        negative_release("retired-bridge", "retired_bridge", &valid, |release| {
            release["manifest"]["console"][0]["presentation"]["kind"] =
                Value::String("isolated".to_owned());
            release["manifest"]["console"][0]["presentation"]["bridge_protocol"] =
                Value::String("lenso.console-bridge.v1".to_owned());
        }),
    ];
    json!({
        "protocol": "lenso.console-contract-vectors.v1",
        "positive": {
            "id": "esm-module-release",
            "expected": "accepted",
            "release": valid,
        },
        "negative": negative,
        "negativeOperations": [
            {
                "id": "config-read-without-field-capability",
                "operation": "config_read",
                "failure": "capability",
                "request": {
                    "context": {
                        "systemId": "system-1",
                        "serviceId": "service-1",
                        "environmentId": "production",
                        "targetServicePrincipal": "spiffe://lenso/service-1",
                        "callerModuleId": "acme/support-console",
                        "delegatedActorSubject": "operator-1",
                        "delegatedAuthorityDigest": digest("f"),
                        "capabilities": []
                    },
                    "moduleId": "acme/support-console",
                    "keys": ["endpoint"]
                }
            }
        ]
    })
}

fn generated_console_schema<T: JsonSchema>(file_name: &str, title: &str) -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T))
        .expect("generated Console schema must serialize");
    let object = schema
        .as_object_mut()
        .expect("generated Console schema root must be an object");
    object.insert(
        "$id".to_owned(),
        Value::String(format!("https://contracts.lenso.local/console/{file_name}")),
    );
    object.insert("title".to_owned(), Value::String(title.to_owned()));
    schema
}

fn sample_console_release() -> ModuleRelease {
    let manifest = ModuleManifest::builder("acme/support-console")
        .capabilities(vec![
            "support.console.read".to_owned(),
            "support.endpoint.read".to_owned(),
            "support.endpoint.write".to_owned(),
        ])
        .config(ModuleConfigContract {
            fields: vec![ModuleConfigField {
                key: "endpoint".to_owned(),
                field_type: ModuleConfigFieldType::String,
                required: true,
                scope: ModuleConfigScope::Service,
                sensitive: false,
                secret_reference: false,
                mutability: ModuleConfigMutability::Reloadable,
                activation: ModuleConfigActivation::Restart,
                read_capability: Some("support.endpoint.read".to_owned()),
                write_capability: Some("support.endpoint.write".to_owned()),
                default: None,
                validation: None,
            }],
        })
        .console(vec![ConsoleSurface {
            name: "support".to_owned(),
            label: "Support".to_owned(),
            route: "/data/support/tickets".to_owned(),
            presentation: ConsoleSurfacePresentation::Esm {
                entry: "support".to_owned(),
            },
            icon: Some("inbox".to_owned()),
            required_capabilities: vec!["support.console.read".to_owned()],
            navigation: None,
        }])
        .build();
    let base_manifest = ModuleManifest::builder("acme/support-console").build();
    let mut release = ModuleRelease::new(
        "acme/support-console",
        "1.2.3",
        base_manifest,
        ModuleDelivery::Linked(LinkedModuleDelivery {
            package: "acme-support-console".to_owned(),
            crate_version: "1.2.3".to_owned(),
            archive_checksum: digest("a"),
            default_features: true,
            features: Vec::new(),
            binding: "support_console".to_owned(),
            attestations: Vec::new(),
            migrations: Vec::new(),
        }),
    )
    .expect("sample Console Module Release base is valid");
    release.manifest = manifest.clone();
    release.manifest_digest =
        crate::digest_json(&manifest).expect("sample Console Manifest is digestible");
    release.compatibility.host_api_requirement = Some("^1.0.0".to_owned());
    release.compatibility.console_ui_requirement = Some("^2.0.0".to_owned());
    release.console_ui_artifact = Some(ConsoleUiArtifact {
        artifact: ArtifactReference {
            locator: "oci://registry.example/acme/support-console-ui@sha256:bbbb".to_owned(),
            digest: digest("b"),
        },
        format: ConsoleUiArtifactFormat::Esm,
        protocol_major: CONSOLE_MODULE_PROTOCOL_MAJOR,
        entry: "assets/support.js".to_owned(),
        entries: vec![
            ConsoleUiArtifactEntry {
                name: "support".to_owned(),
                path: "assets/support.js".to_owned(),
            },
            ConsoleUiArtifactEntry {
                name: "support-style".to_owned(),
                path: "assets/support.css".to_owned(),
            },
        ],
        style_assets: vec![ConsoleUiArtifactStyleAsset {
            path: "assets/support.css".to_owned(),
            order: Some(0),
            media: None,
        }],
        manifest: manifest.console_module_manifest("^1.0.0", "^2.0.0"),
        requested_permissions: Vec::new(),
        provenance: Vec::new(),
    });
    assert!(
        release.validate().is_empty(),
        "sample release must validate"
    );
    release
}

fn negative_release(
    id: &str,
    failure: &str,
    valid: &Value,
    mutate: impl FnOnce(&mut Value),
) -> Value {
    let mut release = valid.clone();
    mutate(&mut release);
    json!({ "id": id, "expected": "rejected", "failure": failure, "release": release })
}

fn digest(value: &str) -> String {
    format!("sha256:{}", value.repeat(64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_contract_vectors_have_one_valid_and_only_invalid_negative_releases() {
        let vectors = console_contract_vectors();
        let positive: ModuleRelease =
            serde_json::from_value(vectors["positive"]["release"].clone())
                .expect("positive Console vector should deserialize");
        assert!(positive.validate().is_empty());

        let negatives = vectors["negative"]
            .as_array()
            .expect("negative Console vectors should be an array");
        assert!(negatives.len() >= 7);
        for vector in negatives {
            let release: ModuleRelease = serde_json::from_value(vector["release"].clone())
                .expect("negative Console vector should deserialize");
            assert!(
                !release.validate().is_empty(),
                "negative vector {} unexpectedly validated",
                vector["id"]
            );
        }
    }
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
