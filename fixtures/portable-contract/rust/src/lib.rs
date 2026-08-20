include!("../../generated/profile.rs");

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn generated_profile_round_trips_portable_values_and_preserves_presence() {
        let request = RoundTripRequest {
            duration: "PT1.5S".to_owned(),
            kind: None,
            local_note: None,
            name: "Ada".to_owned(),
            nullable_map: Some(BTreeMap::from([
                ("first".to_owned(), Some(1)),
                ("second".to_owned(), None),
            ])),
            nullable_values: Some(vec![Some("one".to_owned()), None]),
            optional_note: Some(None),
            payload: "AQI=".to_owned(),
            signed: "-9223372036854775808".to_owned(),
            timestamp: "2026-08-21T12:34:56.123Z".to_owned(),
            unsigned: "18446744073709551615".to_owned(),
            values: vec![1, 2, 3],
        };
        let wire = encode_round_trip_request(&request).expect("request should encode");
        assert_eq!(
            decode_round_trip_request(&wire).expect("request should decode"),
            request
        );

        let missing = decode_round_trip_request(
            r#"{"duration":"PT1S","name":"Ada","payload":"AQI=","signed":"0","timestamp":"2026-08-21T00:00:00Z","unsigned":"0","values":[]}"#,
        )
        .expect("missing optional values should decode");
        assert_eq!(missing.optional_note, None);

        let explicit_null = decode_round_trip_request(
            r#"{"duration":"PT1S","name":"Ada","payload":"AQI=","signed":"0","timestamp":"2026-08-21T00:00:00Z","unsigned":"0","values":[],"optional_note":null}"#,
        )
        .expect("explicit null should decode");
        assert_eq!(explicit_null.optional_note, Some(None));

        let unknown = decode_round_trip_error(r#"{"code":"future","payload":null}"#)
            .expect("unknown errors should decode");
        assert_eq!(
            unknown,
            RoundTripError::Unknown(UnknownDomainError {
                code: "future".to_owned(),
                payload: Some(serde_json::Value::Null),
                extra: BTreeMap::new(),
            })
        );
        let unknown_without_payload = decode_round_trip_error(r#""future_without_payload""#)
            .expect("unknown string errors should decode");
        assert_eq!(
            encode_round_trip_error(&unknown_without_payload)
                .expect("unknown string errors should encode"),
            r#"{"code":"future_without_payload"}"#
        );

        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../../conformance.json"))
                .expect("the shared conformance corpus should be valid JSON");
        for fixture in corpus.as_array().expect("the corpus should be an array") {
            let name = fixture["name"]
                .as_str()
                .expect("fixture should have a name");
            let corpus_value: BTreeMap<String, serde_json::Value> =
                serde_json::from_value(fixture["wire"].clone())
                    .expect("corpus values should be object maps");
            let request_wire = encode_corpus_round_trip_request(&corpus_value)
                .expect("corpus request should encode");
            assert_eq!(
                decode_corpus_round_trip_request(&request_wire)
                    .expect("corpus request should decode"),
                corpus_value
            );
            let response_wire = encode_corpus_round_trip_response(&corpus_value)
                .expect("corpus response should encode");
            assert_eq!(
                decode_corpus_round_trip_response(&response_wire)
                    .expect("corpus response should decode"),
                corpus_value
            );
            let error = RoundTripError::Unknown(UnknownDomainError {
                code: format!("future_{name}"),
                payload: Some(fixture["wire"].clone()),
                extra: BTreeMap::new(),
            });
            let wire = encode_round_trip_error(&error).expect("opaque error should encode");
            assert_eq!(
                decode_round_trip_error(&wire).expect("opaque error should decode"),
                error
            );
        }
    }

    #[test]
    fn generated_profile_preserves_unknown_error_fields() {
        let wire = r#"{"code":"future","payload":{"reason":"later"},"retry_after_ms":2500}"#;
        let error = decode_round_trip_error(wire).expect("unknown error fields should decode");
        assert_eq!(
            error,
            RoundTripError::Unknown(UnknownDomainError {
                code: "future".to_owned(),
                payload: Some(serde_json::json!({"reason": "later"})),
                extra: BTreeMap::from([("retry_after_ms".to_owned(), serde_json::json!(2500),)]),
            })
        );
        assert_eq!(
            encode_round_trip_error(&error).expect("unknown error fields should encode"),
            wire
        );
    }

    #[test]
    fn generated_profile_rejects_unsafe_ordinary_json_integers() {
        let error = decode_round_trip_request(
            r#"{"duration":"PT1S","name":"Ada","payload":"AQI=","signed":"0","timestamp":"2026-08-21T00:00:00Z","unsigned":"0","values":[9007199254740992]}"#,
        )
        .expect_err("ordinary JSON integers must stay in the safe range");
        assert!(error.to_string().contains("unsafe number"));

        let error = decode_round_trip_error(r#"{"code":"future","payload":9007199254740992.5}"#)
            .expect_err("unsafe integer-valued floats must stay in the safe range");
        assert!(error.to_string().contains("unsafe number"));
    }
}
