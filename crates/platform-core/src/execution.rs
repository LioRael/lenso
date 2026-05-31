use crate::context::{ActorContext, CorrelationId, TenantId, TraceContext};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ExecutionId(pub String);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionContext {
    pub execution_id: ExecutionId,
    pub function_name: String,
    pub attempt: u32,
    pub queue: String,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<String>,
    pub actor: ActorContext,
    pub tenant_id: Option<TenantId>,
    pub trace: TraceContext,
    pub deadline: Option<DateTime<Utc>>,
}
