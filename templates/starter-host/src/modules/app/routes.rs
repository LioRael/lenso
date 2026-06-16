use lenso_host::http::{
    ApiErrorResponse, ApiOpenApiRouter, AppContext, AppError, DataResponse, ErrorCode,
    ErrorResponse, HttpRequestContext, Json, JsonBody, OpenApiRouter, Path, RequestContext, State,
    json, routes,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
struct AppStatusResponse {
    status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AppStatusResponseEnvelope)]
struct AppStatusResponseEnvelope {
    data: AppStatusResponse,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateItemRequest {
    title: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct AppItem {
    id: i64,
    title: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AppItemResponseEnvelope)]
struct AppItemResponseEnvelope {
    data: AppItem,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AppItemsResponseEnvelope)]
struct AppItemsResponseEnvelope {
    data: Vec<AppItem>,
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(router())
}

fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(create_item))
        .routes(routes!(get_item))
        .routes(routes!(list_items))
}

#[utoipa::path(
    get,
    path = "/v1/app/status",
    operation_id = "app_status",
    tag = "app",
    responses((
        status = 200,
        description = "App module status",
        body = AppStatusResponseEnvelope,
        content_type = "application/json"
    ))
)]
async fn status() -> Json<DataResponse<AppStatusResponse>> {
    json(AppStatusResponse { status: "ok" })
}

#[utoipa::path(
    post,
    path = "/v1/app/items",
    operation_id = "app_create_item",
    tag = "app",
    request_body(
        content = CreateItemRequest,
        content_type = "application/json",
        description = "Create an app-owned item"
    ),
    responses(
        (
            status = 200,
            description = "Item created",
            body = AppItemResponseEnvelope,
            content_type = "application/json"
        ),
        (
            status = 400,
            description = "Request validation failed",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
async fn create_item(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    JsonBody(input): JsonBody<CreateItemRequest>,
) -> Result<Json<DataResponse<AppItem>>, ApiErrorResponse> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Validation, "item title is required"),
            &request_ctx,
        ));
    }

    let row = sqlx::query("insert into app.items (title) values ($1) returning id, title")
        .bind(title)
        .fetch_one(&ctx.db)
        .await
        .map_err(|error| database_error(error, &request_ctx))?;

    Ok(json(item_from_row(row, &request_ctx)?))
}

#[utoipa::path(
    get,
    path = "/v1/app/items",
    operation_id = "app_list_items",
    tag = "app",
    responses(
        (
            status = 200,
            description = "Recent app-owned items",
            body = AppItemsResponseEnvelope,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
async fn list_items(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<DataResponse<Vec<AppItem>>>, ApiErrorResponse> {
    let rows = sqlx::query("select id, title from app.items order by id desc limit 50")
        .fetch_all(&ctx.db)
        .await
        .map_err(|error| database_error(error, &request_ctx))?;
    let items = rows
        .into_iter()
        .map(|row| item_from_row(row, &request_ctx))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(json(items))
}

#[utoipa::path(
    get,
    path = "/v1/app/items/{id}",
    operation_id = "app_get_item",
    tag = "app",
    params(("id" = i64, Path, description = "App item id")),
    responses(
        (
            status = 200,
            description = "App-owned item",
            body = AppItemResponseEnvelope,
            content_type = "application/json"
        ),
        (
            status = 404,
            description = "Item not found",
            body = ErrorResponse,
            content_type = "application/json"
        ),
        (
            status = 500,
            description = "Internal server error",
            body = ErrorResponse,
            content_type = "application/json"
        )
    )
)]
async fn get_item(
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path(id): Path<i64>,
) -> Result<Json<DataResponse<AppItem>>, ApiErrorResponse> {
    let row = sqlx::query("select id, title from app.items where id = $1")
        .bind(id)
        .fetch_optional(&ctx.db)
        .await
        .map_err(|error| database_error(error, &request_ctx))?
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::NotFound, format!("app item {id} was not found")),
                &request_ctx,
            )
        })?;

    Ok(json(item_from_row(row, &request_ctx)?))
}

fn item_from_row(
    row: sqlx::postgres::PgRow,
    request_ctx: &RequestContext,
) -> Result<AppItem, ApiErrorResponse> {
    Ok(AppItem {
        id: row
            .try_get("id")
            .map_err(|error| database_error(error, request_ctx))?,
        title: row
            .try_get("title")
            .map_err(|error| database_error(error, request_ctx))?,
    })
}

fn database_error(error: sqlx::Error, request_ctx: &RequestContext) -> ApiErrorResponse {
    ApiErrorResponse::with_context(
        AppError::new(ErrorCode::Internal, "App item database operation failed")
            .with_source(error),
        request_ctx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_documents_app_routes() {
        let document = router().to_openapi();

        assert!(document.paths.paths.contains_key("/v1/app/status"));
        let items = document
            .paths
            .paths
            .get("/v1/app/items")
            .expect("items path should be documented");
        assert!(items.get.is_some());
        assert!(items.post.is_some());
        let item = document
            .paths
            .paths
            .get("/v1/app/items/{id}")
            .expect("item detail path should be documented");
        assert!(item.get.is_some());
    }
}
