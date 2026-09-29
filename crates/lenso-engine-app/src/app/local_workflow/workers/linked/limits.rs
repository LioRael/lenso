//! Explicit event budgets; resource-specific timeout policy stays with its owner.
use std::{fs, path::Path};

use anyhow::{Context as _, ensure};
use serde_json::{Value, json};

pub(super) fn load(path: Option<&Path>) -> anyhow::Result<Value> {
    let Some(path) = path else {
        return Ok(json!({}));
    };
    let metadata = fs::metadata(path).context("read Workers Host limits")?;
    ensure!(
        metadata.is_file() && metadata.len() <= 16_384,
        "Workers Host limits file size limit"
    );
    let limits: Value =
        serde_json::from_slice(&fs::read(path)?).context("invalid Workers Host limits JSON")?;
    validate(&limits)?;
    Ok(limits)
}

fn validate(limits: &Value) -> anyhow::Result<()> {
    let values = limits
        .as_object()
        .context("Workers Host limits must be an object")?;
    for (name, value) in values {
        let maximum = match name.as_str() {
            "eventLimitMs"
            | "sessionLimitMs"
            | "cancellationLimitMs"
            | "bodyReadTimeoutMs"
            | "cleanupTimeoutMs" => 2_147_483_647,
            "maxConcurrent" => 32,
            "retirementAdmissionLimit" => 96,
            "maxRequestBodyBytes" | "maxResponseBodyBytes" => 1_048_576,
            "maxRequestHeadBytes" => 16_384,
            "maxOperations" => 128,
            _ => anyhow::bail!("unsupported Workers Host limit `{name}`"),
        };
        ensure!(
            value
                .as_u64()
                .is_some_and(|number| number > 0 && number <= maximum),
            "Workers Host limit `{name}` must be a positive integer no larger than {maximum}"
        );
    }
    Ok(())
}

pub(super) fn split(limits: &Value, explicit_scope: bool) -> (Value, Value) {
    let mut host = limits.clone();
    let mut scope = json!({});
    if explicit_scope {
        for name in ["cleanupTimeoutMs", "maxOperations"] {
            if let Some(value) = host.as_object_mut().and_then(|values| values.remove(name)) {
                scope[name] = value;
            }
        }
    }
    (host, scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_cleanup_budget_is_owned_by_the_scope_factory() {
        let configured = json!({"eventLimitMs":10000,"cleanupTimeoutMs":1000,"maxConcurrent":2});
        validate(&configured).unwrap();
        let (host, scope) = split(&configured, true);
        assert_eq!(host, json!({"eventLimitMs":10000,"maxConcurrent":2}));
        assert_eq!(scope, json!({"cleanupTimeoutMs":1000}));
        assert_eq!(split(&configured, false).0, configured);
        assert_eq!(load(None).unwrap(), json!({}));
        for invalid in [
            json!({"eventLimitMs":0}),
            json!({"maxConcurrent":33}),
            json!({"eventLimitMs":1.5}),
            json!({"unknown":10000}),
        ] {
            assert!(validate(&invalid).is_err());
        }
    }
}
