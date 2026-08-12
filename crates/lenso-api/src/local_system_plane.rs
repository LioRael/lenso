use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use axum::{Extension, Router};
use lenso_service::{
    AuthenticatedServicePrincipal, AuthenticatedTransportBinding, WorkloadCredential,
    WorkloadCredentialRequest, WorkloadIdentityError, WorkloadIdentityErrorCode,
    WorkloadIdentityEvidence, WorkloadIdentityProvider, WorkloadIdentityVerification,
};
use platform_core::{AppConfig, is_local_development_environment};
use platform_system_plane::{
    EnrollmentGrant, SystemPlaneAccess, SystemPlaneRegistryBuilder, SystemPlaneRuntime,
    SystemSandboxEnrollmentAuthorizer,
};
use serde::Deserialize;

pub(crate) const LOCAL_SYSTEM_PLANE_CONFIG_ENV: &str = "LENSO_LOCAL_SYSTEM_PLANE_CONFIG";
const LOCAL_SYSTEM_PLANE_PROTOCOL: &str = "lenso.local-system-plane.v1";
const LOCAL_SYSTEM_PLANE_AUDIENCE: &str = "lenso.local-system-plane";
const LOCAL_TRANSPORT_BINDING: &str = "lenso.loopback-http";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalSystemPlaneConfig {
    protocol: String,
    bearer_token: String,
    enrollment: EnrollmentGrant,
}

#[derive(Debug)]
struct LocalExactBearerProvider {
    token: String,
    console_service_principal: String,
    expires_at_unix_ms: u64,
}

impl WorkloadIdentityProvider for LocalExactBearerProvider {
    fn issue(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Result<WorkloadCredential, WorkloadIdentityError> {
        Err(identity_error(
            WorkloadIdentityErrorCode::ProviderUnavailable,
            "Local exact-bearer identity only verifies the Console credential",
            Some(request.service_principal),
        ))
    }

    fn verify(
        &self,
        token: &str,
        verification: &WorkloadIdentityVerification,
    ) -> Result<AuthenticatedServicePrincipal, WorkloadIdentityError> {
        if token != self.token {
            return Err(identity_error(
                WorkloadIdentityErrorCode::InvalidProof,
                "Local System Plane bearer credential was rejected",
                None,
            ));
        }
        if verification.audience != LOCAL_SYSTEM_PLANE_AUDIENCE {
            return Err(identity_error(
                WorkloadIdentityErrorCode::AudienceMismatch,
                "Local System Plane bearer audience was rejected",
                Some(self.console_service_principal.clone()),
            ));
        }
        if verification.authenticated_transport_binding != LOCAL_TRANSPORT_BINDING {
            return Err(identity_error(
                WorkloadIdentityErrorCode::TransportBindingMismatch,
                "Local System Plane request is not bound to the loopback transport",
                Some(self.console_service_principal.clone()),
            ));
        }
        if verification.now_unix_ms >= self.expires_at_unix_ms {
            return Err(identity_error(
                WorkloadIdentityErrorCode::CredentialExpired,
                "Local System Plane bearer credential has expired",
                Some(self.console_service_principal.clone()),
            ));
        }
        let credential_id = "lenso-local-system-plane".to_owned();
        let key_id = "local-exact-bearer".to_owned();
        Ok(AuthenticatedServicePrincipal {
            service_principal: self.console_service_principal.clone(),
            credential_id: credential_id.clone(),
            issuer: "lenso-local-development".to_owned(),
            audience: verification.audience.clone(),
            expires_at_unix_ms: self.expires_at_unix_ms,
            key_id: key_id.clone(),
            algorithm: "exact-bearer-development-only".to_owned(),
            evidence: WorkloadIdentityEvidence {
                outcome: "authenticated".to_owned(),
                service_principal: Some(self.console_service_principal.clone()),
                credential_id: Some(credential_id),
                key_id: Some(key_id),
            },
        })
    }
}

pub(crate) fn router_from_env(config: &AppConfig) -> Result<Option<Router>> {
    let Some(path) = std::env::var_os(LOCAL_SYSTEM_PLANE_CONFIG_ENV) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let local = read_private_config(&path)?;
    router(config, local).map(Some)
}

fn router(app: &AppConfig, local: LocalSystemPlaneConfig) -> Result<Router> {
    validate_local_boundary(app, &local)?;
    let registry = SystemPlaneRegistryBuilder::new(
        &local.enrollment.managed_service_id,
        &local.enrollment.managed_service_principal,
        &local.enrollment.managed_service_revision,
    )
    .build()
    .map_err(|issues| anyhow::anyhow!("invalid local System Plane Core: {issues:?}"))?;
    let provider = Arc::new(LocalExactBearerProvider {
        token: local.bearer_token,
        console_service_principal: local.enrollment.console_service_principal.clone(),
        expires_at_unix_ms: local.enrollment.expires_at_unix_ms,
    });
    let enrollment = Arc::new(
        SystemSandboxEnrollmentAuthorizer::new("local", local.enrollment)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    );
    let access = SystemPlaneAccess::new(provider, LOCAL_SYSTEM_PLANE_AUDIENCE, enrollment);
    let runtime = Arc::new(SystemPlaneRuntime::new(registry, access));
    let (router, _document) = platform_system_plane::router::<()>(Some(runtime))
        .layer(Extension(AuthenticatedTransportBinding::new(
            LOCAL_TRANSPORT_BINDING,
        )))
        .split_for_parts();
    Ok(router)
}

fn read_private_config(path: &Path) -> Result<LocalSystemPlaneConfig> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect local System Plane config {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("local System Plane config must be a regular file and not a symbolic link");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("local System Plane config must not be accessible by group or others");
        }
    }
    serde_json::from_slice(
        &fs::read(path)
            .with_context(|| format!("read local System Plane config {}", path.display()))?,
    )
    .with_context(|| format!("decode local System Plane config {}", path.display()))
}

fn validate_local_boundary(app: &AppConfig, local: &LocalSystemPlaneConfig) -> Result<()> {
    if local.protocol != LOCAL_SYSTEM_PLANE_PROTOCOL {
        bail!("local System Plane config protocol must be {LOCAL_SYSTEM_PLANE_PROTOCOL}");
    }
    if !is_local_development_environment(&app.service.environment) {
        bail!("local System Plane is forbidden outside local development and tests");
    }
    let host = app
        .http
        .host
        .parse::<IpAddr>()
        .context("local System Plane HTTP_HOST must be a loopback IP address")?;
    if !host.is_loopback() {
        bail!("local System Plane requires a loopback HTTP_HOST");
    }
    if local.enrollment.managed_service_id != app.service.name {
        bail!(
            "local System Plane managed Service must match SERVICE_NAME ({})",
            app.service.name
        );
    }
    if local.enrollment.managed_service_principal
        != format!("service:{}", local.enrollment.managed_service_id)
    {
        bail!("local System Plane managed Service Principal is invalid");
    }
    if local.bearer_token.len() < 32 || local.bearer_token.chars().any(char::is_whitespace) {
        bail!("local System Plane bearer token must be one private value of at least 32 bytes");
    }
    Ok(())
}

fn identity_error(
    code: WorkloadIdentityErrorCode,
    message: &str,
    principal: Option<String>,
) -> WorkloadIdentityError {
    WorkloadIdentityError {
        code,
        message: message.to_owned(),
        evidence: WorkloadIdentityEvidence {
            outcome: "rejected".to_owned(),
            service_principal: principal,
            credential_id: None,
            key_id: Some("local-exact-bearer".to_owned()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use lenso_service::system_plane::EnrollmentPolicyGrant;
    use platform_core::{
        AuthConfig, DatabaseConfig, HttpConfig, ModuleSourcesConfig, RedisConfig, ServiceConfig,
        TelemetryConfig,
    };
    use tower::ServiceExt as _;

    fn app_config(environment: &str, host: &str) -> AppConfig {
        AppConfig {
            service: ServiceConfig {
                name: "taste".to_owned(),
                environment: environment.to_owned(),
            },
            database: DatabaseConfig {
                url: "postgres://unused".to_owned(),
                max_connections: 1,
            },
            redis: RedisConfig::default(),
            http: HttpConfig {
                host: host.to_owned(),
                port: 3000,
                cors_allowed_origins: Vec::new(),
            },
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            module_sources: ModuleSourcesConfig::default(),
            modules: Default::default(),
        }
    }

    fn local_config() -> LocalSystemPlaneConfig {
        LocalSystemPlaneConfig {
            protocol: LOCAL_SYSTEM_PLANE_PROTOCOL.to_owned(),
            bearer_token: "local-console-token-0123456789abcdef".to_owned(),
            enrollment: EnrollmentGrant {
                system_id: "taste-system".to_owned(),
                managed_service_id: "taste".to_owned(),
                managed_service_principal: "service:taste".to_owned(),
                managed_service_revision: "1".to_owned(),
                console_service_principal: "service:lenso-console".to_owned(),
                offer_digest: format!("sha256:{}", "a".repeat(64)),
                receipt_digest: format!("sha256:{}", "c".repeat(64)),
                grant_revision: 1,
                authorization_epoch: 1,
                expires_at_unix_ms: 4_000_000_000_000,
                capabilities: Vec::new(),
                policy: EnrollmentPolicyGrant {
                    policy_id: "local".to_owned(),
                    policy_revision: "1".to_owned(),
                    policy_digest: format!("sha256:{}", "b".repeat(64)),
                },
            },
        }
    }

    #[tokio::test]
    async fn exact_local_bearer_discovers_the_host_core() {
        let local = local_config();
        let token = local.bearer_token.clone();
        let response = router(&app_config("local", "127.0.0.1"), local)
            .unwrap()
            .oneshot(
                Request::get("/system-plane/v1")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn local_system_plane_rejects_any_other_bearer() {
        let response = router(&app_config("local", "127.0.0.1"), local_config())
            .unwrap()
            .oneshot(
                Request::get("/system-plane/v1")
                    .header(header::AUTHORIZATION, "Bearer not-the-local-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn local_system_plane_rejects_production_and_public_bindings() {
        assert!(
            validate_local_boundary(&app_config("production", "127.0.0.1"), &local_config())
                .is_err()
        );
        assert!(validate_local_boundary(&app_config("local", "0.0.0.0"), &local_config()).is_err());
    }
}
