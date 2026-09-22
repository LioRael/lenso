//! Strict, opt-in public `OpenAPI` contract fragments derived from Endpoint types.
//!
//! This module deliberately derives the fragment from the handler's existing
//! `Path`, `QueryParams`, `Json`, response, and `Problem` values. It is carried
//! through the generated Endpoint description as a private extension and is
//! removed after `lenso-openapi-plugin` has checked the authored Operation
//! Object. It is therefore not a second application DTO or a public document
//! extension.

use std::collections::{BTreeMap, BTreeSet};

use lenso_contract_authoring::schema_for;
use schemars::JsonSchema;
use serde_json::{Map, Value, json};

/// Private Operation extension that carries a generated strict contract to the
/// optional `OpenAPI` document Plugin.
pub const OPENAPI_CONTRACT_EXTENSION: &str = "x-lenso-contract";

/// Error returned while deriving a strict `OpenAPI` contract from typed values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenApiContractError {
    detail: String,
}

impl OpenApiContractError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for OpenApiContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for OpenApiContractError {}

/// Result produced by one generated Endpoint contract factory.
pub type OpenApiContractResult = Result<Value, OpenApiContractError>;

/// Deferred strict contract factory stored alongside one immutable Endpoint route.
pub type OpenApiContractFactory = fn() -> OpenApiContractResult;

/// Builds the generated `OpenAPI` representation that an authored public
/// Operation must match.
///
/// This builder is normally emitted by `#[endpoint]` for a handler marked with
/// `#[openapi_contract(...)]`. It stays public so explicit `HttpEndpoint`
/// implementations can opt in without inventing a parallel DTO model.
#[derive(Debug)]
pub struct OpenApiContract {
    route_id: &'static str,
    method: &'static str,
    path: &'static str,
    parameters: BTreeMap<(String, String), Value>,
    request_body: Option<Value>,
    success: Option<(u16, Value)>,
    problems: BTreeMap<u16, BTreeSet<String>>,
}

impl OpenApiContract {
    /// Starts a strict contract for one declared Endpoint route.
    #[must_use]
    pub fn new(route_id: &'static str, method: &'static str, path: &'static str) -> Self {
        Self {
            route_id,
            method,
            path,
            parameters: BTreeMap::new(),
            request_body: None,
            success: None,
            problems: BTreeMap::new(),
        }
    }

    /// Derives required path parameters from one `Path<T>` value.
    pub fn path<T: JsonSchema>(mut self) -> Result<Self, OpenApiContractError> {
        self.add_parameters::<T>("path", true)?;
        Ok(self)
    }

    /// Derives query parameters from one `QueryParams<T>` value.
    pub fn query<T: JsonSchema>(mut self) -> Result<Self, OpenApiContractError> {
        self.add_parameters::<T>("query", false)?;
        Ok(self)
    }

    /// Derives a required `application/json` request body from one `Json<T>` value.
    pub fn json_body<T: JsonSchema>(mut self) -> Result<Self, OpenApiContractError> {
        self.set_json_body::<T>(true)?;
        Ok(self)
    }

    /// Derives an optional `application/json` request body from `Option<Json<T>>`.
    pub fn optional_json_body<T: JsonSchema>(mut self) -> Result<Self, OpenApiContractError> {
        self.set_json_body::<T>(false)?;
        Ok(self)
    }

    /// Derives one JSON success response with its known HTTP status.
    pub fn success_json<T: JsonSchema>(
        mut self,
        status: u16,
    ) -> Result<Self, OpenApiContractError> {
        if !(200..=299).contains(&status) {
            return Err(OpenApiContractError::new(format!(
                "strict OpenAPI success status {status} must be between 200 and 299"
            )));
        }
        if self.success.replace((status, schema_for::<T>())).is_some() {
            return Err(OpenApiContractError::new(
                "a strict OpenAPI contract can declare only one success response",
            ));
        }
        Ok(self)
    }

    /// Adds one known RFC 9457 `Problem` response code and status.
    pub fn known_problem(
        mut self,
        status: u16,
        code: impl Into<String>,
    ) -> Result<Self, OpenApiContractError> {
        if !(400..=599).contains(&status) {
            return Err(OpenApiContractError::new(format!(
                "strict OpenAPI problem status {status} must be between 400 and 599"
            )));
        }
        let code = code.into();
        if code.trim().is_empty() {
            return Err(OpenApiContractError::new(
                "a strict OpenAPI problem code must not be empty",
            ));
        }
        self.problems.entry(status).or_default().insert(code);
        Ok(self)
    }

    /// Returns the private Operation extension validated by `lenso-openapi-plugin`.
    pub fn build(self) -> OpenApiContractResult {
        let Some((success_status, success_schema)) = self.success else {
            return Err(OpenApiContractError::new(
                "a strict OpenAPI contract needs one JSON success response",
            ));
        };

        let mut operation = Map::new();
        if !self.parameters.is_empty() {
            operation.insert(
                "parameters".to_owned(),
                Value::Array(self.parameters.into_values().collect()),
            );
        }
        if let Some(request_body) = self.request_body {
            operation.insert("requestBody".to_owned(), request_body);
        }

        let mut responses = Map::new();
        responses.insert(
            success_status.to_string(),
            response("Successful response.", "application/json", &success_schema),
        );
        for (status, codes) in self.problems {
            if status == success_status {
                return Err(OpenApiContractError::new(format!(
                    "known problem status {status} conflicts with the success response"
                )));
            }
            let codes = codes.into_iter().collect::<Vec<_>>();
            let problem_schema = problem_schema(status, &codes);
            responses.insert(
                status.to_string(),
                response(
                    "Known problem response.",
                    "application/problem+json",
                    &problem_schema,
                ),
            );
        }
        operation.insert("responses".to_owned(), Value::Object(responses));

        Ok(json!({
            "route_id": self.route_id,
            "method": self.method,
            "path": self.path,
            "operation": operation,
        }))
    }

    fn add_parameters<T: JsonSchema>(
        &mut self,
        location: &str,
        required_for_all: bool,
    ) -> Result<(), OpenApiContractError> {
        let schema = schema_for::<T>();
        let Some(root) = schema.as_object() else {
            return Err(OpenApiContractError::new(format!(
                "strict OpenAPI {location} parameters must derive from an object schema"
            )));
        };
        let Some(properties) = root.get("properties").and_then(Value::as_object) else {
            return Err(OpenApiContractError::new(format!(
                "strict OpenAPI {location} parameters must derive named object properties"
            )));
        };
        if properties.is_empty() {
            return Err(OpenApiContractError::new(format!(
                "strict OpenAPI {location} parameters must declare at least one property"
            )));
        }
        let required = root
            .get("required")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let definitions = root.get("$defs");

        for (name, property_schema) in properties {
            let mut property_schema = property_schema.clone();
            if contains_local_definition_reference(&property_schema)
                && let Some(definitions) = definitions
                && let Some(object) = property_schema.as_object_mut()
            {
                object.insert("$defs".to_owned(), definitions.clone());
            }
            let key = (location.to_owned(), name.clone());
            if self.parameters.contains_key(&key) {
                return Err(OpenApiContractError::new(format!(
                    "strict OpenAPI contract declares {location} parameter `{name}` more than once"
                )));
            }
            self.parameters.insert(
                key,
                json!({
                    "name": name,
                    "in": location,
                    "required": required_for_all || required.contains(name.as_str()),
                    "schema": property_schema,
                }),
            );
        }
        Ok(())
    }

    fn set_json_body<T: JsonSchema>(&mut self, required: bool) -> Result<(), OpenApiContractError> {
        if self.request_body.is_some() {
            return Err(OpenApiContractError::new(
                "a strict OpenAPI contract can declare only one JSON request body",
            ));
        }
        self.request_body = Some(json!({
            "required": required,
            "content": {
                "application/json": {
                    "schema": schema_for::<T>()
                }
            }
        }));
        Ok(())
    }
}

fn response(description: &str, media_type: &str, schema: &Value) -> Value {
    json!({
        "description": description,
        "content": {
            media_type: {
                "schema": schema
            }
        }
    })
}

fn problem_schema(status: u16, codes: &[String]) -> Value {
    json!({
        "type": "object",
        "required": ["type", "title", "status", "detail", "code"],
        "properties": {
            "type": {"type": "string"},
            "title": {"type": "string"},
            "status": {"const": status},
            "detail": {"type": "string"},
            "code": {"type": "string", "enum": codes}
        },
        "additionalProperties": false
    })
}

fn contains_local_definition_reference(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object
                .get("$ref")
                .and_then(Value::as_str)
                .is_some_and(|reference| reference.starts_with("#/$defs/"))
                || object.values().any(contains_local_definition_reference)
        }
        Value::Array(values) => values.iter().any(contains_local_definition_reference),
        _ => false,
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;

    #[derive(JsonSchema)]
    struct OrderPath {
        order_id: String,
    }

    #[derive(JsonSchema)]
    struct OrderQuery {
        include: Option<String>,
        limit: u16,
    }

    #[derive(JsonSchema)]
    struct CreateOrder {
        name: String,
    }

    #[derive(JsonSchema)]
    struct CreatedOrder {
        id: String,
    }

    #[test]
    fn derives_route_parameters_body_success_and_known_problem_responses() {
        let contract = OpenApiContract::new("orders.create", "POST", "/orders/{order_id}")
            .path::<OrderPath>()
            .unwrap()
            .query::<OrderQuery>()
            .unwrap()
            .json_body::<CreateOrder>()
            .unwrap()
            .success_json::<CreatedOrder>(201)
            .unwrap()
            .known_problem(422, "invalid_order")
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(contract["route_id"], "orders.create");
        assert_eq!(contract["method"], "POST");
        assert_eq!(contract["path"], "/orders/{order_id}");
        assert_eq!(
            contract["operation"]["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|parameter| parameter["in"] == "path")
                .count(),
            1
        );
        assert_eq!(
            contract["operation"]["requestBody"]["content"]["application/json"]["schema"]["properties"]
                ["name"]["type"],
            "string"
        );
        assert_eq!(
            contract["operation"]["responses"]["201"]["content"]["application/json"]["schema"]["properties"]
                ["id"]["type"],
            "string"
        );
        assert_eq!(
            contract["operation"]["responses"]["422"]["content"]["application/problem+json"]["schema"]
                ["properties"]["code"]["enum"],
            json!(["invalid_order"])
        );
    }
}
