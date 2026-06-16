use crate::error::AppResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthReport {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
}

#[async_trait]
pub trait HealthCheck: Debug + Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self) -> AppResult<HealthReport>;
}

#[derive(Clone, Default)]
pub struct HealthRegistry {
    checks: Arc<Vec<Arc<dyn HealthCheck>>>,
}

impl Debug for HealthRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HealthRegistry")
            .field("checks", &self.checks.len())
            .finish()
    }
}

impl HealthRegistry {
    pub fn new(checks: Vec<Arc<dyn HealthCheck>>) -> Self {
        Self {
            checks: Arc::new(checks),
        }
    }

    pub fn checks(&self) -> &[Arc<dyn HealthCheck>] {
        &self.checks
    }

    pub async fn check_all(&self) -> Vec<HealthReport> {
        let mut reports = Vec::with_capacity(self.checks.len());
        for check in self.checks.iter() {
            match check.check().await {
                Ok(report) => reports.push(report),
                Err(error) => reports.push(HealthReport {
                    name: check.name().to_owned(),
                    status: HealthStatus::Unhealthy,
                    message: Some(error.public_message),
                }),
            }
        }
        reports
    }
}
