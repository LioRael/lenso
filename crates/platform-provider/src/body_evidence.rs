use platform_core::ProviderHttpBodyEvidence;
use serde_json::Value;

pub(crate) const MAX_PROVIDER_HTTP_BODY_EVIDENCE_BYTES: usize = 64 * 1024;

pub(crate) fn capture_json_body(
    body: Option<&Value>,
    absent_reason: &'static str,
) -> ProviderHttpBodyEvidence {
    let Some(body) = body else {
        return ProviderHttpBodyEvidence::not_applicable(absent_reason);
    };

    let Ok(encoded) = serde_json::to_vec(body) else {
        return ProviderHttpBodyEvidence::not_captured("serialization_failed", None);
    };
    let raw_bytes = encoded.len();
    if raw_bytes > MAX_PROVIDER_HTTP_BODY_EVIDENCE_BYTES {
        return ProviderHttpBodyEvidence::not_captured("evidence_limit_exceeded", Some(raw_bytes));
    }

    let redacted = redact_json_value(body.clone());
    let Ok(redacted_encoded) = serde_json::to_vec(&redacted) else {
        return ProviderHttpBodyEvidence::not_captured("serialization_failed", None);
    };
    if redacted_encoded.len() > MAX_PROVIDER_HTTP_BODY_EVIDENCE_BYTES {
        return ProviderHttpBodyEvidence::not_captured(
            "evidence_limit_exceeded",
            Some(redacted_encoded.len()),
        );
    }

    ProviderHttpBodyEvidence::captured(redacted, raw_bytes)
}

fn redact_json_value(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(redact_json_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if is_sensitive_json_key(&key) {
                        (key, Value::String("[redacted]".to_owned()))
                    } else {
                        (key, redact_json_value(value))
                    }
                })
                .collect(),
        ),
        value => value,
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "credential",
        "email",
    ]
    .iter()
    .any(|unsafe_part| lower.contains(unsafe_part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::ProviderHttpBodyCaptureStatus;
    use serde_json::json;

    #[test]
    fn recursively_redacts_sensitive_fields_before_capture() {
        let body = json!({
            "profile": {
                "email": "alex@example.com",
                "credentials": [{ "access_token": "secret" }],
                "display_name": "Alex"
            }
        });

        let evidence = capture_json_body(Some(&body), "method_without_body");

        assert_eq!(
            evidence.capture_status(),
            ProviderHttpBodyCaptureStatus::Captured
        );
        let captured = evidence.body().expect("body should be captured");
        assert_eq!(captured["profile"]["email"], "[redacted]");
        assert_eq!(captured["profile"]["credentials"], "[redacted]");
        assert_eq!(captured["profile"]["display_name"], "Alex");
    }

    #[test]
    fn omits_body_larger_than_the_evidence_limit() {
        let body = json!({ "value": "x".repeat(MAX_PROVIDER_HTTP_BODY_EVIDENCE_BYTES) });
        let evidence_limit = i64::try_from(MAX_PROVIDER_HTTP_BODY_EVIDENCE_BYTES)
            .expect("body evidence limit should fit in i64");

        let evidence = capture_json_body(Some(&body), "method_without_body");

        assert_eq!(
            evidence.capture_status(),
            ProviderHttpBodyCaptureStatus::NotCaptured
        );
        assert_eq!(evidence.capture_reason(), Some("evidence_limit_exceeded"));
        assert!(
            evidence
                .observed_bytes()
                .is_some_and(|bytes| bytes > evidence_limit)
        );
    }

    #[test]
    fn redaction_cannot_expand_the_persisted_copy_beyond_the_limit() {
        let evidence_limit = i64::try_from(MAX_PROVIDER_HTTP_BODY_EVIDENCE_BYTES)
            .expect("body evidence limit should fit in i64");
        let body = Value::Object(
            (0..3_000)
                .map(|index| (format!("secret_{index}"), Value::String(String::new())))
                .collect(),
        );
        assert!(serde_json::to_vec(&body).expect("JSON should encode").len() < 64 * 1024);

        let evidence = capture_json_body(Some(&body), "method_without_body");

        assert_eq!(
            evidence.capture_status(),
            ProviderHttpBodyCaptureStatus::NotCaptured
        );
        assert_eq!(evidence.capture_reason(), Some("evidence_limit_exceeded"));
        assert!(
            evidence
                .observed_bytes()
                .is_some_and(|bytes| bytes > evidence_limit)
        );
    }

    #[test]
    fn records_an_explicit_absence_reason() {
        let evidence = capture_json_body(None, "method_without_body");

        assert_eq!(
            evidence.capture_status(),
            ProviderHttpBodyCaptureStatus::NotApplicable
        );
        assert_eq!(evidence.capture_reason(), Some("method_without_body"));
        assert_eq!(evidence.observed_bytes(), None);
    }
}
