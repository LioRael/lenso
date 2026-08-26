use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::DefinitionResolutionError;
use crate::RequestAdmissionPlan;

/// One App-owner request-admission policy for an exact derived binding.
///
/// This does not select or create a binding. The consumer requirement and provider endpoint
/// remain package-owned facts, and ordinary binding closure remains resolver-owned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BindingPolicy {
    consumer: String,
    capability_id: String,
    provider: String,
    admission: RequestAdmissionPlan,
}

impl BindingPolicy {
    pub fn new(
        consumer: impl Into<String>,
        capability_id: impl Into<String>,
        provider: impl Into<String>,
        admission: RequestAdmissionPlan,
    ) -> Self {
        Self {
            consumer: consumer.into(),
            capability_id: capability_id.into(),
            provider: provider.into(),
            admission,
        }
    }

    #[must_use]
    pub fn with_limits(
        consumer: impl Into<String>,
        capability_id: impl Into<String>,
        provider: impl Into<String>,
        queue_capacity: usize,
        max_concurrency: usize,
    ) -> Self {
        Self::new(
            consumer,
            capability_id,
            provider,
            RequestAdmissionPlan::new(queue_capacity, max_concurrency),
        )
    }

    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub const fn admission(&self) -> RequestAdmissionPlan {
        self.admission
    }
}

pub(super) fn index_binding_policies(
    policies: &[BindingPolicy],
) -> Result<BTreeMap<(String, String, String), RequestAdmissionPlan>, DefinitionResolutionError> {
    let mut indexed = BTreeMap::new();
    for policy in policies {
        let key = (
            policy.consumer.clone(),
            policy.capability_id.clone(),
            policy.provider.clone(),
        );
        if indexed.insert(key.clone(), policy.admission).is_some() {
            return Err(DefinitionResolutionError::DuplicateBindingPolicy {
                consumer: key.0,
                capability_id: key.1,
                provider: key.2,
            });
        }
    }
    Ok(indexed)
}
