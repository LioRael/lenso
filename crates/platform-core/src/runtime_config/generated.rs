use crate::db::DbPool;
use crate::error::AppResult;
use crate::runtime_config::descriptor::{
    RuntimeConfigGeneratedValue, RuntimeConfigRegistry, RuntimeConfigVisibilityCondition,
};
use crate::runtime_config::snapshot::RuntimeConfigSnapshot;
use crate::runtime_config::store::upsert_value_if_missing;
use rand_core::{OsRng, RngCore};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Initialize generated values whose conditions are true and whose stored row is
/// missing or JSON null.
pub async fn initialize_generated_values(
    pool: &DbPool,
    registry: &RuntimeConfigRegistry,
    service_key: &str,
    stored: &BTreeMap<(String, String), Value>,
    actor: Option<&str>,
) -> AppResult<Vec<(String, String)>> {
    let snapshot = RuntimeConfigSnapshot::resolve(registry, service_key, stored);
    let mut generated = Vec::new();
    for descriptor in registry.iter() {
        let descriptor_service = descriptor.scope.as_service_key();
        if descriptor_service != "*" && descriptor_service != service_key {
            continue;
        }
        let Some(RuntimeConfigGeneratedValue::Secret { bytes, when }) = &descriptor.generated
        else {
            continue;
        };
        if !condition_matches(when, service_key, &snapshot) {
            continue;
        }
        let value = serde_json::json!(generate_secret(*bytes));
        if upsert_value_if_missing(pool, descriptor_service, &descriptor.key, &value, actor)
            .await?
            .is_some()
        {
            generated.push((descriptor_service.to_owned(), descriptor.key.to_owned()));
        }
    }
    Ok(generated)
}

fn condition_matches(
    condition: &RuntimeConfigVisibilityCondition,
    service_key: &str,
    snapshot: &RuntimeConfigSnapshot,
) -> bool {
    match condition {
        RuntimeConfigVisibilityCondition::Equals {
            service,
            key,
            value,
        } if *service == "*" || *service == service_key => {
            snapshot.raw(key).is_some_and(|current| current == value)
        }
        RuntimeConfigVisibilityCondition::Equals { .. } => false,
    }
}

fn generate_secret(bytes: usize) -> String {
    let mut raw = vec![0_u8; bytes];
    OsRng.fill_bytes(&mut raw);
    let mut out = String::with_capacity(bytes * 2);
    for byte in raw {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}
