#[allow(clippy::wildcard_imports)]
use super::*;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use platform_core::{AppContext, AppError, ErrorCode};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const API_BINARY_NAME: &str = "lenso-api";
const SIGNAL_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestartLaunch {
    SelfSpawned,
    RequiresSupervisor,
}

#[utoipa::path(
    post,
    path = "/admin/system/restart",
    operation_id = "admin_system_restart",
    tag = "admin-system",
    params(
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 202, description = "Service restart requested", body = AdminServiceRestartResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn restart_service(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<(StatusCode, Json<AdminServiceRestartResponse>), ApiErrorResponse> {
    let actor = admin_audit_label(&admin);
    let launch = schedule_restart().map_err(|source| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Internal, "failed to schedule service restart")
                .with_source(source),
            &request_ctx,
        )
    })?;
    let shutdown = ctx.shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SIGNAL_DELAY).await;
        shutdown.signal();
    });
    tracing::warn!(
        actor = %actor,
        requires_supervisor = launch == RestartLaunch::RequiresSupervisor,
        "service restart requested"
    );
    Ok((
        StatusCode::ACCEPTED,
        Json(AdminServiceRestartResponse {
            status: match launch {
                RestartLaunch::SelfSpawned => "restart_scheduled",
                RestartLaunch::RequiresSupervisor => "shutdown_requested",
            }
            .to_owned(),
            service: "api".to_owned(),
            requires_supervisor: launch == RestartLaunch::RequiresSupervisor,
        }),
    ))
}

fn schedule_restart() -> std::io::Result<RestartLaunch> {
    let Some(exe) = current_lenso_api_exe() else {
        return Ok(RestartLaunch::RequiresSupervisor);
    };
    spawn_delayed_restart(&exe)?;
    Ok(RestartLaunch::SelfSpawned)
}

fn current_lenso_api_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    (stem == API_BINARY_NAME).then_some(exe)
}

#[cfg(unix)]
fn spawn_delayed_restart(exe: &Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new("nohup");
    command
        .arg("sh")
        .arg("-c")
        .arg("sleep 0.7; exec \"$1\"")
        .arg("lenso-api-restart")
        .arg(exe)
        .current_dir(std::env::current_dir()?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.process_group(0);
    command.spawn()?;
    Ok(())
}

#[cfg(not(unix))]
fn spawn_delayed_restart(exe: &Path) -> std::io::Result<()> {
    Command::new(exe)
        .current_dir(std::env::current_dir()?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_api_test_binary_requires_supervisor() {
        assert!(matches!(
            schedule_restart().expect("restart schedule check"),
            RestartLaunch::RequiresSupervisor
        ));
    }
}
