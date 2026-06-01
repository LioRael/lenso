use axum::Json;
use axum::extract::State;
use platform_core::{AppContext, HealthStatus};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
}

pub async fn livez() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: HealthStatus::Healthy,
    })
}

pub async fn readyz(State(ctx): State<AppContext>) -> Json<HealthResponse> {
    let reports = ctx.health.check_all().await;
    let status = if reports
        .iter()
        .any(|report| report.status == HealthStatus::Unhealthy)
    {
        HealthStatus::Unhealthy
    } else {
        HealthStatus::Healthy
    };

    Json(HealthResponse { status })
}
