use std::collections::{BTreeMap, BTreeSet};

use lenso_capability_http_endpoint::DescribeResponse;
use lenso_capability_http_endpoint::OPENAPI_CONTRACT_EXTENSION;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::OpenApiConfig;
use crate::contract;

const METHODS: &[&str] = &[
    "delete", "get", "head", "options", "patch", "post", "put", "trace",
];

#[derive(Serialize)]
struct Document<'a> {
    openapi: &'static str,
    #[serde(rename = "jsonSchemaDialect")]
    json_schema_dialect: &'static str,
    info: Info<'a>,
    paths: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(skip_serializing_if = "slice_is_empty")]
    servers: &'a [crate::config::OpenApiServer],
    #[serde(skip_serializing_if = "Option::is_none")]
    components: Option<&'a Map<String, Value>>,
}

fn slice_is_empty<T>(slice: &&[T]) -> bool {
    slice.is_empty()
}

#[derive(Serialize)]
struct Info<'a> {
    title: &'a str,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

pub(crate) fn assemble(
    config: &OpenApiConfig,
    descriptions: Vec<DescribeResponse>,
) -> Result<Vec<u8>, String> {
    let mut paths = BTreeMap::<String, BTreeMap<String, Value>>::new();
    let mut operation_ids = BTreeSet::new();
    for route in descriptions
        .into_iter()
        .flat_map(|description| description.routes)
    {
        let (path, path_parameters) = openapi_path(&route.route_id, &route.path)?;
        let method = route.method.trim().to_ascii_lowercase();
        if !METHODS.contains(&method.as_str()) {
            return Err(format!(
                "route {} uses HTTP method {} which OpenAPI 3.1 cannot describe",
                route.route_id, route.method
            ));
        }
        if route.route_id.trim().is_empty() {
            return Err("an OpenAPI route has an empty route ID".to_owned());
        }
        if !operation_ids.insert(route.route_id.clone()) {
            return Err(format!("duplicate OpenAPI operationId {}", route.route_id));
        }
        let mut operation = route
            .openapi
            .unwrap_or_default()
            .into_iter()
            .collect::<Map<_, _>>();
        let strict_contract = operation.remove(OPENAPI_CONTRACT_EXTENSION);
        if operation.contains_key("operationId") {
            return Err(format!(
                "route {} must not override its generated operationId",
                route.route_id
            ));
        }
        match operation.get("responses") {
            Some(Value::Object(_)) => {}
            Some(_) => {
                return Err(format!(
                    "route {} has a non-object OpenAPI responses field",
                    route.route_id
                ));
            }
            None => {
                operation.insert(
                    "responses".to_owned(),
                    json!({"default": {"description": "Undocumented response."}}),
                );
            }
        }
        if let Some(strict_contract) = strict_contract {
            contract::validate(
                config,
                &route.route_id,
                &route.method,
                &route.path,
                &operation,
                &strict_contract,
            )?;
        }
        ensure_path_parameters(&route.route_id, &path_parameters, &mut operation)?;
        operation.insert("operationId".to_owned(), Value::String(route.route_id));
        let methods = paths.entry(path.clone()).or_default();
        if methods
            .insert(method.clone(), Value::Object(operation))
            .is_some()
        {
            return Err(format!(
                "duplicate OpenAPI route {} {}",
                method.to_ascii_uppercase(),
                path
            ));
        }
    }
    if paths.is_empty() {
        return Err("OpenAPI requires at least one explicitly bound Endpoint".to_owned());
    }
    let document = Document {
        openapi: "3.1.0",
        json_schema_dialect: "https://json-schema.org/draft/2020-12/schema",
        info: Info {
            title: config.title(),
            version: config.version(),
            description: config.description(),
        },
        paths,
        servers: config.servers(),
        components: config.components(),
    };
    let mut bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("OpenAPI document serialization failed: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn openapi_path(route_id: &str, path: &str) -> Result<(String, Vec<String>), String> {
    if !path.starts_with('/') || path.contains(['?', '#']) {
        return Err(format!(
            "route {route_id} does not use an absolute OpenAPI path"
        ));
    }
    let mut rendered = String::with_capacity(path.len());
    let mut parameters = Vec::new();
    let mut remainder = path;
    while let Some(start) = remainder.find('{') {
        rendered.push_str(&remainder[..start]);
        let after_start = &remainder[start + 1..];
        let Some(end) = after_start.find('}') else {
            return Err(format!("route {route_id} has an unclosed path parameter"));
        };
        let declared = &after_start[..end];
        if declared.contains('{') {
            return Err(format!("route {route_id} has a nested path parameter"));
        }
        let name = declared.strip_prefix('*').unwrap_or(declared);
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return Err(format!("route {route_id} has an invalid path parameter"));
        }
        if parameters.iter().any(|parameter| parameter == name) {
            return Err(format!("route {route_id} repeats path parameter {name}"));
        }
        parameters.push(name.to_owned());
        rendered.push('{');
        rendered.push_str(name);
        rendered.push('}');
        remainder = &after_start[end + 1..];
    }
    if remainder.contains('}') {
        return Err(format!(
            "route {route_id} has an unmatched path parameter terminator"
        ));
    }
    rendered.push_str(remainder);
    Ok((rendered, parameters))
}

fn ensure_path_parameters(
    route_id: &str,
    path_parameters: &[String],
    operation: &mut Map<String, Value>,
) -> Result<(), String> {
    if path_parameters.is_empty() {
        return Ok(());
    }
    let parameters = operation
        .entry("parameters".to_owned())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| format!("route {route_id} has a non-array OpenAPI parameters field"))?;
    let mut declared = BTreeSet::new();
    for parameter in parameters.iter() {
        let Some(parameter) = parameter.as_object() else {
            return Err(format!(
                "route {route_id} has a non-object OpenAPI parameter"
            ));
        };
        if parameter.get("in").and_then(Value::as_str) != Some("path") {
            continue;
        }
        let Some(name) = parameter.get("name").and_then(Value::as_str) else {
            return Err(format!(
                "route {route_id} has a path parameter without a name"
            ));
        };
        if parameter.get("required").and_then(Value::as_bool) != Some(true) {
            return Err(format!(
                "route {route_id} path parameter {name} must be required"
            ));
        }
        declared.insert(name.to_owned());
    }
    for name in path_parameters {
        if declared.insert(name.clone()) {
            parameters.push(json!({
                "name": name,
                "in": "path",
                "required": true,
                "schema": {"type": "string"}
            }));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use std::collections::BTreeMap;

    use lenso_capability_http_endpoint::{
        DescribeResponse, DescribeResponseRoutesItem, JsonSchema, OPENAPI_CONTRACT_EXTENSION,
        OpenApiContract,
    };
    use serde_json::{Value, json};

    use super::assemble;
    use crate::OpenApiConfig;

    #[test]
    fn assembles_deterministic_openapi_for_explicit_descriptions() {
        let config = config(json!({
            "title": "Orders",
            "version": "1.0.0",
            "servers": [{"url": "https://api.example.test"}],
            "components": {
                "schemas": {
                    "Order": {"type": "object"}
                }
            }
        }));
        let descriptions = vec![description(vec![route(
            "orders.read",
            "GET",
            "/orders/{order_id}",
            Some(json!({
                "summary": "Read an order",
                "responses": {
                    "200": {
                        "description": "Order",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/Order"}
                            }
                        }
                    }
                }
            })),
        )])];

        let first = assemble(&config, descriptions.clone()).unwrap();
        let second = assemble(&config, descriptions).unwrap();
        assert_eq!(first, second);
        let document: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(document["info"]["title"], "Orders");
        assert_eq!(
            document["paths"]["/orders/{order_id}"]["get"]["operationId"],
            "orders.read"
        );
        assert_eq!(
            document["paths"]["/orders/{order_id}"]["get"]["summary"],
            "Read an order"
        );
        assert_eq!(
            document["paths"]["/orders/{order_id}"]["get"]["parameters"][0]["name"],
            "order_id"
        );
        assert_eq!(
            document["paths"]["/orders/{order_id}"]["get"]["parameters"][0]["required"],
            true
        );
        assert_eq!(document["servers"][0]["url"], "https://api.example.test");
        assert_eq!(document["components"]["schemas"]["Order"]["type"], "object");
    }

    #[test]
    fn routes_without_a_strict_contract_remain_valid_and_receive_a_default_response() {
        let document = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![route(
                "health.read",
                "GET",
                "/health",
                None,
            )])],
        )
        .unwrap();
        let document: Value = serde_json::from_slice(&document).unwrap();
        assert_eq!(
            document["paths"]["/health"]["get"]["responses"]["default"]["description"],
            "Undocumented response."
        );
    }

    #[test]
    fn rejects_duplicate_operation_ids_and_non_openapi_methods() {
        let duplicate = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![
                route("orders.read", "GET", "/orders/{id}", None),
                route("orders.read", "GET", "/orders/{id}/detail", None),
            ])],
        )
        .unwrap_err();
        assert!(duplicate.contains("duplicate OpenAPI operationId"));

        let unsupported = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![route(
                "search.query",
                "SEARCH",
                "/search",
                None,
            )])],
        )
        .unwrap_err();
        assert!(unsupported.contains("OpenAPI 3.1 cannot describe"));
    }

    #[test]
    fn rejects_provider_metadata_that_overrides_authoritative_fields() {
        let operation_id = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![route(
                "orders.read",
                "GET",
                "/orders/{id}",
                Some(json!({"operationId": "drifted"})),
            )])],
        )
        .unwrap_err();
        assert!(operation_id.contains("must not override"));

        let responses = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![route(
                "orders.read",
                "GET",
                "/orders/{id}",
                Some(json!({"responses": []})),
            )])],
        )
        .unwrap_err();
        assert!(responses.contains("non-object OpenAPI responses"));

        let optional_path_parameter = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![route(
                "orders.read",
                "GET",
                "/orders/{id}",
                Some(json!({
                    "parameters": [{
                        "name": "id",
                        "in": "path",
                        "required": false,
                        "schema": {"type": "string"}
                    }]
                })),
            )])],
        )
        .unwrap_err();
        assert!(optional_path_parameter.contains("must be required"));
    }

    #[test]
    fn normalizes_matchit_catch_all_paths_for_openapi() {
        let document = assemble(
            &OpenApiConfig::default(),
            vec![description(vec![route(
                "assets.read",
                "GET",
                "/assets/{*path}",
                None,
            )])],
        )
        .unwrap();
        let document: Value = serde_json::from_slice(&document).unwrap();
        assert!(document["paths"].get("/assets/{path}").is_some());
        assert_eq!(
            document["paths"]["/assets/{path}"]["get"]["parameters"][0]["name"],
            "path"
        );
    }

    #[derive(JsonSchema)]
    struct OrderPath {
        order_id: String,
    }

    #[derive(JsonSchema)]
    struct OrderQuery {
        include: Option<bool>,
    }

    #[derive(JsonSchema)]
    struct CreateOrder {
        id: String,
    }

    #[derive(JsonSchema)]
    struct CreatedOrder {
        id: String,
    }

    #[test]
    fn strict_contract_matches_typed_route_and_is_not_emitted() {
        let contract = strict_contract();
        let document = assemble(
            &OpenApiConfig::default(),
            vec![strict_description(
                contract.clone(),
                strict_operation(&contract),
            )],
        )
        .unwrap();
        let document: Value = serde_json::from_slice(&document).unwrap();
        let operation = &document["paths"]["/orders/{order_id}"]["post"];

        assert_eq!(operation["operationId"], "orders.create");
        assert!(operation.get(OPENAPI_CONTRACT_EXTENSION).is_none());
        assert_eq!(
            operation["requestBody"]["content"]["application/json"]["schema"]["properties"]["id"]["type"],
            "string"
        );
        assert_eq!(
            operation["responses"]["422"]["content"]["application/problem+json"]["schema"]["properties"]
                ["code"]["enum"],
            json!(["invalid_order"])
        );
    }

    #[test]
    fn strict_contract_rejects_route_identity_method_and_path_drift() {
        for (field, value) in [
            ("route_id", json!("orders.changed")),
            ("method", json!("GET")),
            ("path", json!("/changed/{order_id}")),
        ] {
            let mut contract = strict_contract();
            contract[field] = value;
            let error = assemble(
                &OpenApiConfig::default(),
                vec![strict_description(
                    contract.clone(),
                    strict_operation(&contract),
                )],
            )
            .unwrap_err();
            assert!(error.contains(&format!("drifted {field}")), "{error}");
        }
    }

    #[test]
    fn strict_contract_rejects_parameter_body_success_and_problem_drift() {
        let contract = strict_contract();

        let mut path = strict_operation(&contract);
        path["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|parameter| parameter["in"] == "path")
            .unwrap()["name"] = json!("other_id");
        let error = assemble(
            &OpenApiConfig::default(),
            vec![strict_description(contract.clone(), path)],
        )
        .unwrap_err();
        assert!(error.contains("path or query parameters"), "{error}");

        let mut query = strict_operation(&contract);
        query["parameters"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|parameter| parameter["in"] == "query")
            .unwrap()["schema"]["type"] = json!("integer");
        let error = assemble(
            &OpenApiConfig::default(),
            vec![strict_description(contract.clone(), query)],
        )
        .unwrap_err();
        assert!(error.contains("query parameter `include`"), "{error}");

        let mut body = strict_operation(&contract);
        body["requestBody"]["content"]["application/json"]["schema"]["properties"]["id"]["type"] =
            json!("integer");
        let error = assemble(
            &OpenApiConfig::default(),
            vec![strict_description(contract.clone(), body)],
        )
        .unwrap_err();
        assert!(error.contains("JSON request body"), "{error}");

        let mut success = strict_operation(&contract);
        success["responses"]["201"]["content"]["application/json"]["schema"]["properties"]["id"]
            ["type"] = json!("integer");
        let error = assemble(
            &OpenApiConfig::default(),
            vec![strict_description(contract.clone(), success)],
        )
        .unwrap_err();
        assert!(error.contains("response 201"), "{error}");

        let mut problem = strict_operation(&contract);
        problem["responses"]["422"]["content"]["application/problem+json"]["schema"]["properties"]
            ["code"]["enum"] = json!(["other_error"]);
        let error = assemble(
            &OpenApiConfig::default(),
            vec![strict_description(contract.clone(), problem)],
        )
        .unwrap_err();
        assert!(error.contains("response 422"), "{error}");
    }

    fn config(value: Value) -> OpenApiConfig {
        serde_json::from_value(value).unwrap()
    }

    fn description(routes: Vec<DescribeResponseRoutesItem>) -> DescribeResponse {
        DescribeResponse { routes }
    }

    fn route(
        route_id: &str,
        method: &str,
        path: &str,
        openapi: Option<Value>,
    ) -> DescribeResponseRoutesItem {
        DescribeResponseRoutesItem {
            method: method.to_owned(),
            openapi: openapi.map(|value| {
                value
                    .as_object()
                    .expect("test OpenAPI metadata must be an object")
                    .clone()
                    .into_iter()
                    .collect::<BTreeMap<_, _>>()
            }),
            path: path.to_owned(),
            route_id: route_id.to_owned(),
        }
    }

    fn strict_contract() -> Value {
        OpenApiContract::new("orders.create", "POST", "/orders/{order_id}")
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
            .unwrap()
    }

    fn strict_operation(contract: &Value) -> Value {
        contract["operation"].clone()
    }

    fn strict_description(contract: Value, operation: Value) -> DescribeResponse {
        let mut operation = operation;
        operation
            .as_object_mut()
            .unwrap()
            .insert(OPENAPI_CONTRACT_EXTENSION.to_owned(), contract);
        description(vec![route(
            "orders.create",
            "POST",
            "/orders/{order_id}",
            Some(operation),
        )])
    }
}
