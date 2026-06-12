use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct DataResponse<T>
where
    T: Serialize,
{
    pub data: T,
}

pub fn json<T>(data: T) -> Json<DataResponse<T>>
where
    T: Serialize,
{
    Json(DataResponse { data })
}
