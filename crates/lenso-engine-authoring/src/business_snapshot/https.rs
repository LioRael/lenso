use std::{collections::BTreeSet, fmt, io::Read as _, time::Instant};

use anyhow::{Context as _, bail, ensure};
use url::Url;

use super::{
    BusinessSnapshotSourceBinding, BusinessSnapshotSourceId, MAX_DOCUMENT_BYTES, SourceDocument,
    SourceLocation, VersionedBusinessSnapshot, snapshot_from_document,
};
use crate::{
    archive_download::{checked_url, public_resolve, restricted_https_agent_builder},
    configuration_snapshot::validate_etag,
};

/// An opaque cursor tied to one exact endpoint and Host-selected source.
#[derive(Clone, Eq, PartialEq)]
pub struct BusinessSnapshotCursor {
    endpoint: String,
    source: BusinessSnapshotSourceId,
    etag: String,
}

impl BusinessSnapshotCursor {
    pub fn source(&self) -> &BusinessSnapshotSourceId {
        &self.source
    }

    fn validate_for(&self, source: &HttpsBusinessSnapshotSource) -> anyhow::Result<()> {
        validate_etag(&self.etag)?;
        ensure!(
            self.endpoint == source.url.as_str() && self.source == source.source,
            "business snapshot ETag cursor belongs to a different source"
        );
        Ok(())
    }
}

impl fmt::Debug for BusinessSnapshotCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSnapshotCursor")
            .field("source", &self.source)
            .field("endpoint", &"<redacted>")
            .field("etag", &"<redacted>")
            .finish()
    }
}

pub(super) enum PollOutcome {
    Updated {
        snapshot: Box<VersionedBusinessSnapshot>,
        cursor: Option<BusinessSnapshotCursor>,
    },
    NotModified {
        cursor: BusinessSnapshotCursor,
        observed_at: Instant,
    },
}

/// One fetched HTTPS result; callers must pass it to the Host authority for admission.
pub struct BusinessSnapshotPoll {
    pub(super) outcome: PollOutcome,
}

impl BusinessSnapshotPoll {
    pub fn revision(&self) -> Option<u64> {
        match &self.outcome {
            PollOutcome::Updated { snapshot, .. } => Some(snapshot.revision()),
            PollOutcome::NotModified { .. } => None,
        }
    }
}

impl fmt::Debug for BusinessSnapshotPoll {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSnapshotPoll")
            .field("revision", &self.revision())
            .finish_non_exhaustive()
    }
}

/// Fixed-origin HTTPS reader with no redirects, proxies, or private DNS results.
pub struct HttpsBusinessSnapshotSource {
    url: Url,
    source: BusinessSnapshotSourceId,
    admitted_origins: BTreeSet<String>,
}

impl HttpsBusinessSnapshotSource {
    pub fn new(
        url: &str,
        source: BusinessSnapshotSourceId,
        admitted_origins: &[String],
    ) -> anyhow::Result<Self> {
        ensure!(
            !admitted_origins.is_empty() && admitted_origins.len() <= 16,
            "expected 1 to 16 business snapshot origins"
        );
        let mut origins = BTreeSet::new();
        for origin in admitted_origins {
            let origin = checked_url(origin, "business snapshot")?;
            ensure!(
                origin.path() == "/" && origin.query().is_none(),
                "business snapshot policy requires an origin, not a path or query"
            );
            origins.insert(origin.origin().ascii_serialization());
        }
        let url = checked_url(url, "business snapshot")?;
        ensure!(
            origins.contains(&url.origin().ascii_serialization()),
            "business snapshot origin is not admitted by the Host"
        );
        Ok(Self {
            url,
            source,
            admitted_origins: origins,
        })
    }

    pub fn source(&self) -> &BusinessSnapshotSourceId {
        &self.source
    }

    pub fn binding(&self) -> BusinessSnapshotSourceBinding {
        BusinessSnapshotSourceBinding {
            source: self.source.clone(),
            location: SourceLocation::Https(self.url.as_str().to_owned()),
        }
    }

    pub fn poll(
        &self,
        previous: Option<&BusinessSnapshotCursor>,
    ) -> anyhow::Result<BusinessSnapshotPoll> {
        let agent = restricted_https_agent_builder()
            .resolver(public_resolve)
            .build();
        self.poll_with_agent(previous, &agent)
    }

    fn poll_with_agent(
        &self,
        previous: Option<&BusinessSnapshotCursor>,
        agent: &ureq::Agent,
    ) -> anyhow::Result<BusinessSnapshotPoll> {
        ensure!(
            self.admitted_origins
                .contains(&self.url.origin().ascii_serialization()),
            "business snapshot origin is not admitted by the Host"
        );
        if let Some(cursor) = previous {
            cursor.validate_for(self)?;
        }
        let mut request = agent
            .get(self.url.as_str())
            .set("Accept", "application/json")
            .set("Accept-Encoding", "identity");
        if let Some(cursor) = previous {
            request = request.set("If-None-Match", &cursor.etag);
        }
        let response = match request.call() {
            Ok(response) if response.status() == 304 => {
                return not_modified(previous, &response);
            }
            Ok(response) => response,
            Err(ureq::Error::Status(304, response)) => {
                return not_modified(previous, &response);
            }
            Err(_) => bail!("business snapshot HTTPS request failed"),
        };
        ensure!(
            response.status() == 200,
            "business snapshot response must be HTTP 200 or 304"
        );
        ensure!(
            response
                .header("Content-Encoding")
                .is_none_or(|value| value.eq_ignore_ascii_case("identity")),
            "encoded business snapshot responses are not accepted"
        );
        if let Some(length) = response.header("Content-Length") {
            ensure!(
                length
                    .parse::<u64>()
                    .is_ok_and(|length| length <= MAX_DOCUMENT_BYTES),
                "business snapshot response length exceeds its bound"
            );
        }
        let cursor = response
            .header("ETag")
            .map(|etag| {
                validate_etag(etag)?;
                Ok::<_, anyhow::Error>(BusinessSnapshotCursor {
                    endpoint: self.url.as_str().to_owned(),
                    source: self.source.clone(),
                    etag: etag.to_owned(),
                })
            })
            .transpose()?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read business snapshot HTTPS response")?;
        ensure!(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_DOCUMENT_BYTES,
            "business snapshot response exceeds its bound"
        );
        let document: SourceDocument = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("business snapshot HTTPS response is invalid JSON"))?;
        Ok(BusinessSnapshotPoll {
            outcome: PollOutcome::Updated {
                snapshot: Box::new(snapshot_from_document(self.binding(), document)?),
                cursor,
            },
        })
    }
}

fn not_modified(
    previous: Option<&BusinessSnapshotCursor>,
    response: &ureq::Response,
) -> anyhow::Result<BusinessSnapshotPoll> {
    let previous =
        previous.context("business snapshot returned HTTP 304 without an ETag cursor")?;
    if let Some(etag) = response.header("ETag") {
        validate_etag(etag)?;
        ensure!(
            etag == previous.etag,
            "business snapshot HTTP 304 changed its ETag"
        );
    }
    Ok(BusinessSnapshotPoll {
        outcome: PollOutcome::NotModified {
            cursor: previous.clone(),
            observed_at: Instant::now(),
        },
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write as _,
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use serde::Deserialize;
    use serde_json::json;

    use super::*;
    use crate::{
        BusinessSnapshotAcceptance, BusinessSnapshotAuthority, BusinessSnapshotAuthorization,
        BusinessSnapshotObjectId, VersionedBusinessSnapshot,
    };

    #[derive(Deserialize)]
    struct ExcerptPolicy {
        excerpt_limit: u32,
    }

    #[test]
    fn real_tls_poll_preserves_etag_and_304_only_renews_the_accepted_revision() {
        let document = json!({
            "schema": "lenso.business-snapshot.v1",
            "revision": 7,
            "object": {
                "plugin_id": "company.notes",
                "instance_key": "default",
                "object_key": "excerpt-policy"
            },
            "value": { "excerpt_limit": 96 }
        })
        .to_string()
        .into_bytes();
        let server = serve_two_polls(document);
        let source_id = BusinessSnapshotSourceId::new("https_poll", "operator-settings").unwrap();
        let source = HttpsBusinessSnapshotSource::new(
            &server.url,
            source_id.clone(),
            &[Url::parse(&server.url)
                .unwrap()
                .origin()
                .ascii_serialization()],
        )
        .unwrap();
        let object =
            BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap();
        let authority = authority(source.binding(), Duration::from_secs(60));
        let first = source.poll_with_agent(None, &server.agent).unwrap();
        assert_eq!(first.revision(), Some(7));
        assert_eq!(
            authority.accept_poll(first, None).unwrap(),
            BusinessSnapshotAcceptance::Activated
        );
        let cursor = authority.cursor().unwrap().unwrap();
        assert!(!format!("{cursor:?}").contains("revision-7"));
        let other_source = HttpsBusinessSnapshotSource::new(
            &server.url,
            BusinessSnapshotSourceId::new("https_poll", "different-source").unwrap(),
            &[Url::parse(&server.url)
                .unwrap()
                .origin()
                .ascii_serialization()],
        )
        .unwrap();
        assert!(
            other_source
                .poll_with_agent(Some(&cursor), &server.agent)
                .is_err()
        );
        let other_endpoint = HttpsBusinessSnapshotSource::new(
            &format!("{}/other", server.url),
            source_id.clone(),
            &[Url::parse(&server.url)
                .unwrap()
                .origin()
                .ascii_serialization()],
        )
        .unwrap();
        assert!(
            other_endpoint
                .poll_with_agent(Some(&cursor), &server.agent)
                .is_err()
        );
        let second = source
            .poll_with_agent(Some(&cursor), &server.agent)
            .unwrap();
        assert_eq!(second.revision(), None);
        assert_eq!(
            authority.accept_poll(second, Some(7)).unwrap(),
            BusinessSnapshotAcceptance::Unchanged
        );
        assert_eq!(
            authority.capture_request().unwrap().value().excerpt_limit,
            96
        );

        authority
            .accept(
                VersionedBusinessSnapshot::new(
                    source_id,
                    source.binding().location,
                    object,
                    8,
                    json!({ "excerpt_limit": 48 }),
                )
                .unwrap(),
                Some(7),
            )
            .unwrap();
        assert!(
            authority
                .accept_poll(
                    BusinessSnapshotPoll {
                        outcome: PollOutcome::NotModified {
                            cursor,
                            observed_at: Instant::now(),
                        },
                    },
                    Some(8),
                )
                .is_err()
        );
        assert_eq!(
            authority.capture_request().unwrap().value().excerpt_limit,
            48
        );

        server.worker.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .contains("if-none-match: \"revision-7\"")
        );
    }

    #[test]
    fn expired_https_proof_requires_a_full_fetch_instead_of_a_304() {
        let document = json!({
            "schema": "lenso.business-snapshot.v1",
            "revision": 7,
            "object": {
                "plugin_id": "company.notes",
                "instance_key": "default",
                "object_key": "excerpt-policy"
            },
            "value": { "excerpt_limit": 96 }
        })
        .to_string()
        .into_bytes();
        let server = serve_two_polls(document);
        let source_id = BusinessSnapshotSourceId::new("https_poll", "operator-settings").unwrap();
        let source = HttpsBusinessSnapshotSource::new(
            &server.url,
            source_id.clone(),
            &[Url::parse(&server.url)
                .unwrap()
                .origin()
                .ascii_serialization()],
        )
        .unwrap();
        let authority = authority(source.binding(), Duration::from_millis(20));
        authority
            .accept_poll(source.poll_with_agent(None, &server.agent).unwrap(), None)
            .unwrap();
        let old_cursor = authority.cursor().unwrap().unwrap();
        let admitted = authority.capture_request().unwrap();
        thread::sleep(Duration::from_millis(50));
        assert!(authority.cursor().unwrap().is_none());
        let not_modified = source
            .poll_with_agent(Some(&old_cursor), &server.agent)
            .unwrap();
        assert!(authority.accept_poll(not_modified, Some(7)).is_err());
        assert!(authority.capture_request().is_err());
        assert_eq!(admitted.value().excerpt_limit, 96);
        server.worker.join().unwrap();
    }

    fn authority(
        binding: BusinessSnapshotSourceBinding,
        max_stale: Duration,
    ) -> BusinessSnapshotAuthority<ExcerptPolicy> {
        BusinessSnapshotAuthority::new(
            BusinessSnapshotAuthorization::new(
                BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy")
                    .unwrap(),
                binding,
                json!({
                    "type": "object",
                    "properties": { "excerpt_limit": { "type": "integer", "minimum": 16, "maximum": 512 } },
                    "required": ["excerpt_limit"],
                    "additionalProperties": false
                }),
                ["excerpt_limit"],
                max_stale,
            )
            .unwrap(),
        )
    }

    struct PollServer {
        url: String,
        agent: ureq::Agent,
        requests: Arc<Mutex<Vec<String>>>,
        worker: thread::JoinHandle<()>,
    }

    fn serve_two_polls(document: Vec<u8>) -> PollServer {
        let certificate =
            rcgen::generate_simple_self_signed(vec!["business.example".into()]).unwrap();
        let cert = certificate.cert.der().clone();
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der());
        let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key.into())
        .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert).unwrap();
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("https://business.example:{}/snapshot", address.port());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let worker = thread::spawn(move || {
            for request_index in 0..2 {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut stream = rustls::StreamOwned::new(
                    rustls::ServerConnection::new(Arc::new(server_config.clone())).unwrap(),
                    socket,
                );
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") && request.len() < 8_192 {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                captured
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(request).unwrap());
                if request_index == 0 {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"revision-7\"\r\nConnection: close\r\n\r\n",
                        document.len()
                    )
                    .unwrap();
                    stream.write_all(&document).unwrap();
                } else {
                    stream.write_all(b"HTTP/1.1 304 Not Modified\r\nETag: \"revision-7\"\r\nConnection: close\r\n\r\n").unwrap();
                }
                stream.flush().unwrap();
                stream.conn.send_close_notify();
                let _ = stream.flush();
            }
        });
        let agent = restricted_https_agent_builder()
            .resolver(move |_: &str| Ok(vec![address]))
            .tls_config(Arc::new(client_config))
            .build();
        PollServer {
            url,
            agent,
            requests,
            worker,
        }
    }
}
