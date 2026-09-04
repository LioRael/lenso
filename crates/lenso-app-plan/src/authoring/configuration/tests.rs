use serde_json::{Value, json};

use super::resolve_configuration_layers;

fn transport_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {"transport": {"enum": ["stdio", "http"]}, "program": true, "endpoint": true},
        "required": ["transport"],
        "allOf": [{
            "if": {"properties": {"transport": {"const": "stdio"}}, "required": ["transport"]},
            "then": {"properties": {"program": {"type": "string"}}, "required": ["program"]},
            "else": {"properties": {"endpoint": {"type": "string"}}, "required": ["endpoint"]}
        }]
    })
}

#[test]
fn all_of_enforces_every_branch_even_for_secret_references() {
    let schema = json!({"properties": {"token": {
        "x-lenso-sensitive": true, "allOf": [true, false]
    }}});
    assert!(
        resolve_configuration_layers(
            &json!({"token": {"secret_ref": "credential"}}),
            &[],
            Some(&schema),
            "test"
        )
        .is_err()
    );
}

#[test]
fn schema_depth_is_bounded_even_in_inactive_branches() {
    let mut schema = json!(true);
    for _ in 0..66 {
        schema = json!({"if": false, "then": schema});
    }
    let error = resolve_configuration_layers(&json!({}), &[], Some(&schema), "test").unwrap_err();
    assert!(error.detail.contains("exceeds 64 levels"));
}

#[test]
fn conditions_use_effective_overlay_and_preserve_inactive_values() {
    let defaults = json!({"transport": "stdio", "program": "/bin/server"});
    let overlay = json!({"transport": "http", "endpoint": "https://example.com"});
    let effective =
        resolve_configuration_layers(&defaults, &[&overlay], Some(&transport_schema()), "test")
            .unwrap();
    assert_eq!(
        effective,
        json!({"transport": "http", "program": "/bin/server", "endpoint": "https://example.com"})
    );
}

#[test]
fn selected_branch_requires_its_fields() {
    let error = resolve_configuration_layers(
        &json!({"transport": "http"}),
        &[],
        Some(&transport_schema()),
        "test",
    )
    .unwrap_err();
    assert!(
        error
            .detail
            .contains("$.endpoint: required field is missing")
    );
}

#[test]
fn conditional_schema_does_not_relax_unknown_properties() {
    assert!(
        resolve_configuration_layers(
            &json!({"transport": "stdio", "program": "server", "unknown": true}),
            &[],
            Some(&transport_schema()),
            "test"
        )
        .is_err()
    );
}

#[test]
fn malformed_conditions_and_inactive_branches_fail_closed() {
    for schema in [
        json!({"if": {"unsupported": true}, "else": true}),
        json!({"if": false, "then": {"type": 42}}),
        json!({"if": {"required": [1]}, "else": true}),
        json!({"if": {"minimum": "bad"}, "else": true}),
        json!({"if": false, "then": {"properties": {"unused": {"unknown": true}}}}),
        json!({"allOf": []}),
    ] {
        assert!(
            resolve_configuration_layers(&json!({}), &[], Some(&schema), "test").is_err(),
            "{schema}"
        );
    }
}

#[test]
fn boolean_schemas_work_at_root_properties_and_items() {
    for schema in [
        json!(true),
        json!({"properties": {"values": {"items": true}}}),
    ] {
        assert!(
            resolve_configuration_layers(&json!({"values": [1]}), &[], Some(&schema), "test")
                .is_ok()
        );
    }
    for schema in [
        json!(false),
        json!({"properties": {"values": false}}),
        json!({"properties": {"values": {"items": false}}}),
    ] {
        assert!(
            resolve_configuration_layers(&json!({"values": [1]}), &[], Some(&schema), "test")
                .is_err()
        );
    }
}

#[test]
fn size_constraints_in_branches_are_enforced() {
    let schema = json!({"if": true, "then": {"properties": {
        "names": {"type": "array", "maxItems": 2, "uniqueItems": true, "items": {"type": "string", "minLength": 1, "maxLength": 2}},
        "limit": {"minimum": 1, "maximum": 5}
    }}});
    assert!(
        resolve_configuration_layers(
            &json!({"names": ["你好"], "limit": 5}),
            &[],
            Some(&schema),
            "test"
        )
        .is_ok()
    );
    for value in [
        json!({"names": ["a", "a"]}),
        json!({"names": ["abc"]}),
        json!({"names": [""]}),
        json!({"names": ["a", "b", "c"]}),
        json!({"limit": 6}),
    ] {
        assert!(resolve_configuration_layers(&value, &[], Some(&schema), "test").is_err());
    }
}

#[test]
fn object_conditions_ignore_non_objects_and_orphan_branches_are_not_applied() {
    let schema = json!({"then": false, "else": false, "properties": {"value": {
        "if": {"required": ["key"]}, "then": {"const": 1}, "else": false
    }}});
    assert!(resolve_configuration_layers(&json!({"value": 1}), &[], Some(&schema), "test").is_ok());
}
