use futures::executor::block_on;
use lenso_capability_http_endpoint::{
    DescribeRequest, EndpointProvider, Json, JsonSchema, OPENAPI_CONTRACT_EXTENSION, Path,
    QueryParams, endpoint,
    response::{Problem, StatusCode},
};
use lenso_kernel::{CancellationToken, InvocationContext};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default)]
struct PublicOrders;

#[endpoint(standalone)]
impl PublicOrders {
    #[post("orders.create", "/orders/{account_id}")]
    #[openapi({
        summary: "Create an order",
        responses: {
            "201": { description: "Created" }
        }
    })]
    #[openapi_contract(
        success = 201,
        errors = [(422, "invalid_order")]
    )]
    async fn create(
        &self,
        Path(path): Path<OrderPath>,
        QueryParams(query): QueryParams<CreateQuery>,
        Json(input): Json<CreateOrder>,
    ) -> Result<(StatusCode, Json<CreatedOrder>), Problem> {
        std::future::ready(()).await;
        let _ = (&path.account_id, &query.dry_run);
        Ok((StatusCode::CREATED, Json(CreatedOrder { id: input.id })))
    }
}

#[derive(Deserialize, JsonSchema)]
struct OrderPath {
    account_id: String,
}

#[derive(Deserialize, JsonSchema)]
struct CreateQuery {
    dry_run: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct CreateOrder {
    id: String,
}

#[derive(JsonSchema, Serialize)]
struct CreatedOrder {
    id: String,
}

#[test]
fn strict_contract_is_derived_from_the_same_typed_endpoint_declaration() {
    let description = block_on(PublicOrders.describe(
        InvocationContext::new(1, None, CancellationToken::new()),
        DescribeRequest {},
    ))
    .unwrap()
    .unwrap();
    let operation = description.routes[0].openapi.as_ref().unwrap();
    let contract = &operation[OPENAPI_CONTRACT_EXTENSION];

    assert_eq!(contract["route_id"], "orders.create");
    assert_eq!(contract["method"], "POST");
    assert_eq!(contract["path"], "/orders/{account_id}");
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
            ["id"]["type"],
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
        serde_json::json!(["invalid_order"])
    );
}
