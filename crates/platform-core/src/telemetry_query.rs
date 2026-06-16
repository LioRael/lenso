use crate::error::AppResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TelemetrySpan {
    pub id: String,
    pub name: String,
    pub status: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub attributes: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelemetrySpanQuery {
    pub correlation_id: Option<String>,
    pub story_id: Option<String>,
    pub function_run_id: Option<String>,
    pub outbox_event_id: Option<String>,
}

impl TelemetrySpanQuery {
    pub fn by_correlation_id(correlation_id: impl Into<String>) -> Self {
        Self {
            correlation_id: Some(correlation_id.into()),
            ..Self::default()
        }
    }

    pub fn by_function_run_id(function_run_id: impl Into<String>) -> Self {
        Self {
            function_run_id: Some(function_run_id.into()),
            ..Self::default()
        }
    }

    pub fn by_outbox_event_id(outbox_event_id: impl Into<String>) -> Self {
        Self {
            outbox_event_id: Some(outbox_event_id.into()),
            ..Self::default()
        }
    }
}

#[async_trait]
pub trait TelemetrySpanProvider: Debug + Send + Sync {
    async fn query_spans(&self, query: TelemetrySpanQuery) -> AppResult<Vec<TelemetrySpan>>;
}

#[derive(Debug, Default)]
pub struct NoopTelemetrySpanProvider;

#[async_trait]
impl TelemetrySpanProvider for NoopTelemetrySpanProvider {
    async fn query_spans(&self, _query: TelemetrySpanQuery) -> AppResult<Vec<TelemetrySpan>> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryTelemetrySpanProvider {
    spans: Arc<Vec<TelemetrySpan>>,
}

impl InMemoryTelemetrySpanProvider {
    pub fn new(spans: impl Into<Vec<TelemetrySpan>>) -> Self {
        Self {
            spans: Arc::new(spans.into()),
        }
    }
}

#[async_trait]
impl TelemetrySpanProvider for InMemoryTelemetrySpanProvider {
    async fn query_spans(&self, query: TelemetrySpanQuery) -> AppResult<Vec<TelemetrySpan>> {
        Ok(self
            .spans
            .iter()
            .filter(|span| span_matches_query(span, &query))
            .cloned()
            .collect())
    }
}

fn span_matches_query(span: &TelemetrySpan, query: &TelemetrySpanQuery) -> bool {
    let selectors = [
        query
            .correlation_id
            .as_deref()
            .map(|value| ("lenso.correlation_id", value)),
        query
            .story_id
            .as_deref()
            .map(|value| ("lenso.story_id", value)),
        query
            .function_run_id
            .as_deref()
            .map(|value| ("lenso.function_run_id", value)),
        query
            .outbox_event_id
            .as_deref()
            .map(|value| ("lenso.outbox_event_id", value)),
    ];

    let selected = selectors.into_iter().flatten().collect::<Vec<_>>();
    if selected.is_empty() {
        return false;
    }

    selected
        .iter()
        .all(|(key, expected)| span_attribute(span, key) == Some(*expected))
}

fn span_attribute<'a>(span: &'a TelemetrySpan, key: &str) -> Option<&'a str> {
    span.attributes.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_provider_queries_by_correlation_id() {
        let provider = InMemoryTelemetrySpanProvider::new([
            test_span(
                "span_a",
                serde_json::json!({ "lenso.correlation_id": "corr_a" }),
            ),
            test_span(
                "span_b",
                serde_json::json!({ "lenso.correlation_id": "corr_b" }),
            ),
        ]);

        let spans = provider
            .query_spans(TelemetrySpanQuery::by_correlation_id("corr_a"))
            .await
            .expect("query should succeed");

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].id, "span_a");
    }

    #[tokio::test]
    async fn in_memory_provider_queries_by_function_run_id() {
        let provider = InMemoryTelemetrySpanProvider::new([
            test_span(
                "span_a",
                serde_json::json!({ "lenso.function_run_id": "fnrun_a" }),
            ),
            test_span(
                "span_b",
                serde_json::json!({ "lenso.outbox_event_id": "evt_b" }),
            ),
        ]);

        let spans = provider
            .query_spans(TelemetrySpanQuery::by_function_run_id("fnrun_a"))
            .await
            .expect("query should succeed");

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].id, "span_a");
    }

    fn test_span(id: &str, attributes: Value) -> TelemetrySpan {
        TelemetrySpan {
            attributes,
            ended_at: "2026-05-31T00:00:01Z"
                .parse()
                .expect("timestamp should parse"),
            id: id.to_owned(),
            name: id.to_owned(),
            started_at: "2026-05-31T00:00:00Z"
                .parse()
                .expect("timestamp should parse"),
            status: Some("ok".to_owned()),
        }
    }
}
