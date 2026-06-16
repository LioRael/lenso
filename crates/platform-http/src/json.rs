use crate::{ApiErrorResponse, HttpRequestContext};
use axum::extract::{FromRequest, Json, Request};
use platform_core::AppError;
use platform_core::error::ErrorDetail;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, Copy, Default)]
pub struct JsonBody<T>(pub T);

impl<S, T> FromRequest<S> for JsonBody<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiErrorResponse;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let context = request.extensions().get::<HttpRequestContext>().cloned();

        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| {
                let error = AppError::validation(
                    "Request validation failed",
                    vec![ErrorDetail {
                        field: None,
                        reason: json_rejection_reason(&rejection).to_owned(),
                    }],
                );

                match context {
                    Some(ctx) => ApiErrorResponse::with_context(error, &ctx),
                    None => error.into(),
                }
            })
    }
}

fn json_rejection_reason(rejection: &axum::extract::rejection::JsonRejection) -> &'static str {
    match rejection {
        axum::extract::rejection::JsonRejection::JsonDataError(_) => {
            "Request body contains invalid JSON data"
        }
        axum::extract::rejection::JsonRejection::JsonSyntaxError(_) => {
            "Request body contains malformed JSON"
        }
        axum::extract::rejection::JsonRejection::MissingJsonContentType(_) => {
            "Request body must be JSON"
        }
        axum::extract::rejection::JsonRejection::BytesRejection(_) => {
            "Request body could not be read"
        }
        _ => "Request body is invalid",
    }
}
