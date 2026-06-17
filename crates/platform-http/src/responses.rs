use axum::Json;
use serde::Serialize;

pub fn json<T>(body: T) -> Json<T>
where
    T: Serialize,
{
    Json(body)
}
