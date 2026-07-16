# Select SPIFFE/SPIRE as the First Production Workload Identity

Lenso selects SPIFFE X.509-SVIDs plus JWT-SVIDs, supplied through the SPIFFE
Workload API by SPIRE or another conforming implementation, as its first
production Workload Identity integration. X.509-SVID mutual TLS authenticates
the connection; the audience-limited JWT-SVID authenticates the application
request and must name the same peer SPIFFE ID. Lenso maps only
`spiffe://<trust-domain>/service/<service-id>` to the stable
`service:<service-id>` Principal.

## Decision record

| Dimension | SPIFFE Workload API with X.509-SVID and JWT-SVID | Kubernetes projected Service Account tokens | Cloud-specific workload federation |
| --- | --- | --- | --- |
| SPIFFE compatibility | Native SPIFFE IDs, SVID profiles, trust domains, and federation. | Can supply short-lived OIDC tokens, but the identity and trust-domain model is Kubernetes-specific unless another component translates it. | Strong issuer-specific identity, but portability and SPIFFE compatibility require a separate mapping layer. |
| Issuer operations | SPIRE or another conforming implementation owns attestation, registration, CA/JWT signing keys, bundles, and revocation. Lenso consumes the local Workload API and never becomes a CA. | Kubernetes owns token projection and rotation; operators still need a peer verifier and an independent authenticated-transport design. | The cloud control plane owns issuance and rotation; each cloud needs its own verifier, subject mapping, and recovery runbook. |
| Credential lifetime and rotation | The Workload API streams X.509-SVID and bundle changes; live sources apply rotations to new handshakes. JWT-SVID lifetime remains issuer policy and is bounded by the caller request. | Projected tokens rotate, but rotation is bearer-token-only and is not automatically tied to peer TLS identity. | Short-lived credentials are available, but lifetime and refresh behavior differ by provider. |
| Transport binding | X.509-SVID mTLS proves the peer SPIFFE ID; Lenso accepts a JWT-SVID only when its subject equals that authenticated peer ID. | Requires a separate mesh, certificate issuer, or proof-of-possession mechanism. | Often binds to provider APIs rather than arbitrary Service-to-Service HTTP or gRPC; an additional mTLS design is still required. |
| Target-environment burden | Requires a SPIFFE Workload API implementation and workload registration. SPIRE supports process, VM, container, and Kubernetes attestation, while small teams may defer it until production separation needs justify the agent/server footprint. | Lowest burden for Kubernetes-only teams, but makes Kubernetes an architectural requirement and leaves transport binding incomplete. | Low burden inside one chosen cloud, but raises migration and hybrid-environment cost. |
| Failure recovery | Sources reconnect and atomically replace SVIDs and bundles. Existing cached X.509 material and already issued JWT-SVIDs remain usable until expiry; new issuance fails closed when the local Workload API is unavailable. | Kubelet/control-plane recovery is environment-specific, and token verification still depends on cached issuer keys. | Recovery follows each provider's regional and metadata-service behavior, producing multiple operational contracts. |

The SPIFFE Workload API and X.509-SVID validation/rotation behavior follow the
upstream [Workload API](https://spiffe.io/docs/latest/spiffe-specs/spiffe_workload_api/)
and [X.509-SVID](https://spiffe.io/docs/latest/spiffe-specs/x509-svid/)
specifications. The alternatives follow Kubernetes
[projected ServiceAccount token](https://kubernetes.io/docs/tasks/configure-pod-container/configure-service-account/#serviceaccount-token-volume-projection)
and cloud workload-federation models.

## Boundaries and consequences

- `SpiffeWorkloadIdentityProvider` implements the existing provider boundary.
  The original synchronous `issue` signature remains source-compatible for
  existing providers; `issue_async` is the production extension used because
  SPIFFE issuance calls the local Workload API. Verification uses the locally
  cached JWT bundle and remains independent of Runtime Console, Host, and System
  Plane availability.
- `SpiffeWorkloadIdentityConfig` binds one stable Service Principal to one exact
  SPIFFE ID and selects that identity when a workload is entitled to more than
  one SVID. IP addresses, hostnames, replicas, regions, and failure domains never
  become Service identity.
- Production composition builds rustls client and server configurations from
  the provider's live `X509Source` and passes the authenticated peer SPIFFE ID as
  `AuthenticatedTransportBinding`. Request headers cannot supply that binding.
- SPIRE registration entries, trust bundles, Workload API sockets, private keys,
  production credential rotation, and production mutations are operator-owned
  Approval Boundaries. Lenso stores none of those secret values.
- Verification evidence records outcome, Service Principal, JWT digest, and key
  ID. It never records a JWT-SVID, private key, certificate key material, or join
  token.
- CI provisions a short-lived SPIRE server and agent only when
  `LENSO_SPIFFE_TEST_INFRASTRUCTURE_APPROVED=true`. The proof creates two
  temporary registrations, performs a real mTLS HTTP call, waits for X.509-SVID
  rotation, repeats the call without rebuilding the Service, and then destroys
  the test infrastructure.
- The development-only System Sandbox provider remains the dependency-free
  local path and still rejects production configuration. Its credentials are
  not accepted by the SPIFFE production provider.
