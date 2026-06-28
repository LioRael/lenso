use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOperationMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_probe: Option<ServiceOperationSafeProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<ServiceOperationIdempotency>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOperationSafeProbe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_status: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceOperationIdempotency {
    None,
    Idempotent,
    RequiresKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AdminAction, AdminActionDangerLevel, ModuleHttpMethod, ModuleHttpRoute,
        RuntimeFunctionDeclaration,
    };
    use serde_json::json;

    #[test]
    fn operation_metadata_serializes_on_route_runtime_and_action() {
        let route = ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/tickets".to_owned(),
            capability: Some("support_ticket.tickets.read".to_owned()),
            display_name: Some("List tickets".to_owned()),
            story_title: Some("Tickets listed".to_owned()),
            operation: Some(ServiceOperationMetadata {
                operation_id: Some("support-ticket/http/GET:/tickets".to_owned()),
                summary: Some("List tickets".to_owned()),
                safe_probe: Some(ServiceOperationSafeProbe {
                    method: Some("GET".to_owned()),
                    path: Some("/tickets".to_owned()),
                    input: None,
                    expect_status: Some(200),
                }),
                ..ServiceOperationMetadata::default()
            }),
        };
        let route_json = serde_json::to_value(route).unwrap();
        assert_eq!(
            route_json["operation"]["operationId"],
            "support-ticket/http/GET:/tickets"
        );
        assert_eq!(route_json["operation"]["safeProbe"]["expectStatus"], 200);

        let function = RuntimeFunctionDeclaration {
            name: "support-ticket.escalate-ticket.v1".to_owned(),
            version: 1,
            queue: "support-ticket".to_owned(),
            input_schema: None,
            retry_policy: None,
            operation: Some(ServiceOperationMetadata {
                idempotency: Some(ServiceOperationIdempotency::RequiresKey),
                timeout_ms: Some(2_000),
                ..ServiceOperationMetadata::default()
            }),
        };
        let function_json = serde_json::to_value(function).unwrap();
        assert_eq!(function_json["operation"]["idempotency"], "requires_key");
        assert_eq!(function_json["operation"]["timeoutMs"], 2000);

        let action = AdminAction {
            name: "assign_ticket".to_owned(),
            label: "Assign ticket".to_owned(),
            capability: "support_ticket.tickets.write".to_owned(),
            input_schema: None,
            confirmation: None,
            danger_level: AdminActionDangerLevel::Low,
            operation: Some(ServiceOperationMetadata {
                output_schema: Some(json!({ "type": "object" })),
                ..ServiceOperationMetadata::default()
            }),
        };
        let action_json = serde_json::to_value(action).unwrap();
        assert_eq!(action_json["operation"]["outputSchema"]["type"], "object");
    }
}
