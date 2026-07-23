use http::{Method, StatusCode};
use lenso_service::{
    DirectHttpRequest, DirectHttpResponse, DirectHttpServerBinding, SpiffeWorkloadIdentityConfig,
    SpiffeWorkloadIdentityProvider, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityErrorCode, WorkloadIdentityProvider,
    WorkloadIdentityVerification, generate_direct_http_bindings,
};
use rustls::pki_types::ServerName;
use serde_json::json;
use spiffe_rustls::{LocalOnly, authorizer, mtls_client, mtls_server};
use spiffe_rustls_tokio::{TlsAcceptor, TlsConnector};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::timeout,
};

#[test]
fn spiffe_config_maps_only_stable_service_principals() {
    let config = SpiffeWorkloadIdentityConfig::new(
        "unix:///run/spire/sockets/agent.sock",
        "lenso.example",
        "service:support",
    )
    .unwrap();

    assert_eq!(config.service_principal(), "service:support");
    assert_eq!(
        config.spiffe_id().to_string(),
        "spiffe://lenso.example/service/support"
    );

    for coordinate in ["127.0.0.1", "support.local", "replica-7", "region-a"] {
        let error = SpiffeWorkloadIdentityConfig::new(
            "unix:///run/spire/sockets/agent.sock",
            "lenso.example",
            coordinate,
        )
        .unwrap_err();
        assert_eq!(error.code, WorkloadIdentityErrorCode::InvalidRequest);
        assert_eq!(error.evidence.outcome, "invalid_service_principal");
    }

    let missing_endpoint =
        SpiffeWorkloadIdentityConfig::new("", "lenso.example", "service:support").unwrap_err();
    assert_eq!(missing_endpoint.evidence.outcome, "invalid_spiffe_endpoint");
}

fn bindings() -> lenso_service::DirectHttpBindings {
    let document: serde_json::Value = serde_yaml::from_str(include_str!(
        "../fixtures/contracts/v2/support-http.v1.yaml"
    ))
    .unwrap();
    generate_direct_http_bindings("support-http", "v1", &document).unwrap()
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[derive(Debug)]
struct FixedVerificationTimeProvider {
    inner: Arc<SpiffeWorkloadIdentityProvider>,
    now_unix_ms: u64,
}

impl WorkloadIdentityProvider for FixedVerificationTimeProvider {
    fn issue(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Result<lenso_service::WorkloadCredential, lenso_service::WorkloadIdentityError> {
        self.inner.issue(request)
    }

    fn verify(
        &self,
        token: &str,
        verification: &WorkloadIdentityVerification,
    ) -> Result<lenso_service::AuthenticatedServicePrincipal, lenso_service::WorkloadIdentityError>
    {
        self.inner.verify(
            token,
            &WorkloadIdentityVerification::new(
                &verification.audience,
                &verification.authenticated_transport_binding,
                self.now_unix_ms,
            ),
        )
    }
}

async fn real_http_call(
    server: Arc<DirectHttpServerBinding>,
    acceptor: TlsAcceptor,
    connector: TlsConnector,
    credential: &str,
    expected_caller: &str,
    expected_server: &str,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let credential = credential.to_owned();
    let expected_caller = expected_caller.to_owned();
    let server_task = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let (mut tls, peer) = acceptor.accept(tcp).await.unwrap();
        let peer_id = peer.spiffe_id().expect("mTLS peer must have a SPIFFE ID");
        assert_eq!(peer_id.to_string(), expected_caller);

        let mut request_bytes = Vec::new();
        let mut chunk = [0_u8; 2_048];
        while !request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = tls.read(&mut chunk).await.unwrap();
            assert!(read > 0, "HTTP request ended before its headers");
            request_bytes.extend_from_slice(&chunk[..read]);
        }
        let request_text = String::from_utf8(request_bytes).unwrap();
        let bearer = request_text
            .lines()
            .find_map(|line| line.strip_prefix("Authorization: Bearer "))
            .expect("HTTP request must carry a JWT-SVID");
        let response = server
            .handle(
                DirectHttpRequest::new(Method::GET, "/v1/tickets/42")
                    .with_deadline(now_ms() + 30_000)
                    .with_workload_credential(bearer)
                    .with_authenticated_transport_binding(peer_id.to_string()),
            )
            .await;
        let status = response.status.as_u16();
        tls.write_all(
            format!("HTTP/1.1 {status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
        tls.shutdown().await.unwrap();
        response.status
    });

    let (mut tls, peer) = connector
        .connect_addr(address, ServerName::try_from("support.internal").unwrap())
        .await
        .unwrap();
    assert_eq!(
        peer.spiffe_id()
            .expect("mTLS server must have a SPIFFE ID")
            .to_string(),
        expected_server
    );
    tls.write_all(
        format!(
            "GET /v1/tickets/42 HTTP/1.1\r\nHost: support.internal\r\nAuthorization: Bearer {credential}\r\nConnection: close\r\n\r\n"
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut response = Vec::new();
    tls.read_to_end(&mut response).await.unwrap();
    assert!(
        String::from_utf8(response)
            .unwrap()
            .starts_with("HTTP/1.1 200")
    );
    assert_eq!(server_task.await.unwrap(), StatusCode::OK);
}

#[tokio::test]
async fn spire_authenticates_real_http_and_rotates_without_plane_dependencies() {
    if std::env::var("LENSO_SPIFFE_TEST_INFRASTRUCTURE_APPROVED").as_deref() != Ok("true") {
        eprintln!(
            "skipping SPIFFE production proof: LENSO_SPIFFE_TEST_INFRASTRUCTURE_APPROVED=true is not set"
        );
        return;
    }
    let endpoint = std::env::var("SPIFFE_ENDPOINT_SOCKET")
        .expect("approved SPIFFE test infrastructure must expose its Workload API socket");
    let ticketing = Arc::new(
        timeout(
            Duration::from_secs(10),
            SpiffeWorkloadIdentityProvider::connect(
                SpiffeWorkloadIdentityConfig::new(&endpoint, "lenso.test", "service:ticketing")
                    .unwrap(),
            ),
        )
        .await
        .expect("ticketing SPIFFE provider connection must complete")
        .unwrap(),
    );
    let support = Arc::new(
        timeout(
            Duration::from_secs(10),
            SpiffeWorkloadIdentityProvider::connect(
                SpiffeWorkloadIdentityConfig::new(&endpoint, "lenso.test", "service:support")
                    .unwrap(),
            ),
        )
        .await
        .expect("support SPIFFE provider connection must complete")
        .unwrap(),
    );

    let ticketing_spiffe_id = ticketing.config().spiffe_id().to_string();
    let support_spiffe_id = support.config().spiffe_id().to_string();
    let credential = ticketing
        .issue_async(WorkloadCredentialRequest::new(
            "service:ticketing",
            "service:support",
            &ticketing_spiffe_id,
            now_ms(),
            60_000,
        ))
        .await
        .unwrap();

    let wrong_audience = ticketing
        .issue_async(WorkloadCredentialRequest::new(
            "service:ticketing",
            "service:other",
            &ticketing_spiffe_id,
            now_ms(),
            60_000,
        ))
        .await
        .unwrap();
    assert_eq!(
        support
            .verify(
                &wrong_audience.token,
                &WorkloadIdentityVerification::new(
                    "service:support",
                    &ticketing_spiffe_id,
                    now_ms(),
                ),
            )
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::AudienceMismatch
    );
    assert_eq!(
        support
            .verify(
                &credential.token,
                &WorkloadIdentityVerification::new(
                    "service:support",
                    &ticketing_spiffe_id,
                    credential.expires_at_unix_ms,
                ),
            )
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::CredentialExpired
    );
    let mut tampered = credential.token.clone();
    tampered.push('x');
    assert_eq!(
        support
            .verify(
                &tampered,
                &WorkloadIdentityVerification::new(
                    "service:support",
                    &ticketing_spiffe_id,
                    now_ms(),
                ),
            )
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::InvalidProof
    );
    let sandbox = SystemSandboxWorkloadIdentityProvider::new("local", "not-production")
        .unwrap()
        .issue(WorkloadCredentialRequest::new(
            "service:ticketing",
            "service:support",
            &ticketing_spiffe_id,
            now_ms(),
            30_000,
        ))
        .unwrap();
    let development_error = support
        .verify(
            &sandbox.token,
            &WorkloadIdentityVerification::new("service:support", &ticketing_spiffe_id, now_ms()),
        )
        .unwrap_err();
    assert_eq!(
        development_error.code,
        WorkloadIdentityErrorCode::InvalidProof
    );
    assert!(!format!("{:?}", development_error.evidence).contains(&sandbox.token));

    let handled = Arc::new(AtomicUsize::new(0));
    let handled_by_server = Arc::clone(&handled);
    let server = Arc::new(
        DirectHttpServerBinding::new_without_workload_identity(bindings(), move |request| {
            let handled = Arc::clone(&handled_by_server);
            async move {
                handled.fetch_add(1, Ordering::SeqCst);
                assert_eq!(
                    request
                        .authenticated_service_principal
                        .unwrap()
                        .service_principal,
                    "service:ticketing"
                );
                DirectHttpResponse::json(StatusCode::OK, json!({"ticketId": "42"}))
            }
        })
        .with_workload_identity(support.clone(), "service:support"),
    );
    let expired_handled = Arc::clone(&handled);
    let expired_server =
        DirectHttpServerBinding::new_without_workload_identity(bindings(), move |_| {
            let expired_handled = Arc::clone(&expired_handled);
            async move {
                expired_handled.fetch_add(1, Ordering::SeqCst);
                DirectHttpResponse::json(StatusCode::OK, json!({"unexpected": true}))
            }
        })
        .with_workload_identity(
            Arc::new(FixedVerificationTimeProvider {
                inner: support.clone(),
                now_unix_ms: credential.expires_at_unix_ms,
            }),
            "service:support",
        );
    let expired = expired_server
        .handle(
            DirectHttpRequest::new(Method::GET, "/v1/tickets/42")
                .with_deadline(now_ms() + 30_000)
                .with_workload_credential(&credential.token)
                .with_authenticated_transport_binding(&ticketing_spiffe_id),
        )
        .await;
    assert_eq!(expired.status, StatusCode::UNAUTHORIZED);
    assert_eq!(expired.evidence.unwrap().decision, "credential_expired");
    assert_eq!(handled.load(Ordering::SeqCst), 0);

    for rejected_token in [&wrong_audience.token, &tampered, &sandbox.token] {
        let rejected = server
            .handle(
                DirectHttpRequest::new(Method::GET, "/v1/tickets/42")
                    .with_deadline(now_ms() + 30_000)
                    .with_workload_credential(rejected_token)
                    .with_authenticated_transport_binding(&ticketing_spiffe_id),
            )
            .await;
        assert_eq!(rejected.status, StatusCode::UNAUTHORIZED);
        assert_eq!(handled.load(Ordering::SeqCst), 0);
    }

    let acceptor = TlsAcceptor::new(Arc::new(
        mtls_server(support.x509_source())
            .authorize(authorizer::exact([ticketing.config().spiffe_id().clone()]).unwrap())
            .trust_domain_policy(LocalOnly(support.config().trust_domain().clone()))
            .with_alpn_protocols([b"http/1.1"])
            .build()
            .unwrap(),
    ));
    let connector = TlsConnector::new(Arc::new(
        mtls_client(ticketing.x509_source())
            .authorize(authorizer::exact([support.config().spiffe_id().clone()]).unwrap())
            .trust_domain_policy(LocalOnly(ticketing.config().trust_domain().clone()))
            .with_alpn_protocols([b"http/1.1"])
            .build()
            .unwrap(),
    ));

    timeout(
        Duration::from_secs(10),
        real_http_call(
            Arc::clone(&server),
            acceptor.clone(),
            connector.clone(),
            &credential.token,
            &ticketing_spiffe_id,
            &support_spiffe_id,
        ),
    )
    .await
    .expect("initial SPIFFE-authenticated HTTP call must complete");
    assert_eq!(handled.load(Ordering::SeqCst), 1);

    let ticketing_source = ticketing.x509_source();
    let identity_before = ticketing_source.svid().unwrap();
    let certificate_before = identity_before.leaf().as_bytes().to_vec();
    let mut updates = ticketing_source.updated();
    timeout(Duration::from_secs(30), async {
        loop {
            updates.changed().await.unwrap();
            if ticketing_source.svid().unwrap().leaf().as_bytes() != certificate_before {
                break;
            }
        }
    })
    .await
    .expect("SPIRE must rotate the short-lived X.509-SVID");
    let identity_after = ticketing_source.svid().unwrap();
    assert_eq!(identity_after.spiffe_id(), identity_before.spiffe_id());
    assert_ne!(identity_after.leaf().as_bytes(), certificate_before);

    timeout(
        Duration::from_secs(10),
        real_http_call(
            Arc::clone(&server),
            acceptor,
            connector,
            &credential.token,
            &ticketing_spiffe_id,
            &support_spiffe_id,
        ),
    )
    .await
    .expect("SPIFFE-authenticated HTTP call after rotation must complete");
    assert_eq!(handled.load(Ordering::SeqCst), 2);

    drop(expired_server);
    drop(server);
    Arc::try_unwrap(ticketing).unwrap().shutdown().await;
    Arc::try_unwrap(support).unwrap().shutdown().await;
}
