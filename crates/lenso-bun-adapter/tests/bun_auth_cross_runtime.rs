use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ExecutionClassId, ModuleInstancePlan, RestartPolicy,
};
use lenso_bun_adapter::{BunAdapter, BunCapabilityCodec, BunWire};
use lenso_capability_auth::{ActorAssertionIssuer, Validity, audience};
use lenso_capability_greeting::{
    CAPABILITY_ID, DESCRIPTOR_VERSION, GreetError, GreetRequest, GreetResponse, Greeting,
    decode_greet_error, decode_greet_response, encode_greet_request,
};
use lenso_kernel::{
    CancellationToken, DeterministicDriver, ExecutionAdapterCatalog, Kernel, RuntimeFailure,
};

#[derive(Debug)]
struct GreetingCodec;

impl BunCapabilityCodec for GreetingCodec {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &["greet"]
    }

    fn encode_request(
        &self,
        operation: &str,
        request: &dyn std::any::Any,
    ) -> Result<serde_json::Value, RuntimeFailure> {
        if operation != "greet" {
            return Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            });
        }
        let request =
            request
                .downcast_ref::<GreetRequest>()
                .ok_or(RuntimeFailure::ProtocolViolation {
                    capability: CAPABILITY_ID,
                })?;
        let encoded = encode_greet_request(request).map_err(|error| RuntimeFailure::Internal {
            detail: format!("failed to encode Greeting request: {error}"),
        })?;
        serde_json::from_str(&encoded).map_err(|_| RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID,
        })
    }

    fn decode_response(
        &self,
        operation: &str,
        value: serde_json::Value,
    ) -> Result<Box<dyn std::any::Any>, RuntimeFailure> {
        if operation != "greet" {
            return Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            });
        }
        let wire = serde_json::to_string(&value).map_err(|error| RuntimeFailure::Internal {
            detail: format!("failed to encode Greeting response value: {error}"),
        })?;
        Ok(Box::new(decode_greet_response(&wire).map_err(|_| {
            RuntimeFailure::ProtocolViolation {
                capability: CAPABILITY_ID,
            }
        })?))
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: serde_json::Value,
    ) -> Result<Box<dyn std::any::Any>, RuntimeFailure> {
        if operation != "greet" {
            return Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            });
        }
        let wire = serde_json::to_string(&value).map_err(|error| RuntimeFailure::Internal {
            detail: format!("failed to encode Greeting Domain Error value: {error}"),
        })?;
        Ok(Box::new(decode_greet_error(&wire).map_err(|_| {
            RuntimeFailure::ProtocolViolation {
                capability: CAPABILITY_ID,
            }
        })?))
    }
}

fn bun_binary() -> PathBuf {
    std::env::var_os("BUN_BIN").map_or_else(|| PathBuf::from("bun"), PathBuf::from)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/bun")
        .join(name)
}

fn greeting_plan(script: &Path) -> lenso_app_plan::ResolvedAppPlan {
    let endpoint =
        CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, ["greet"]).with_limits(1, 1);
    let provider = ModuleInstancePlan::new("bun-provider", "fixture.bun.greeting")
        .with_entrypoint(script.to_string_lossy())
        .with_execution_class(ExecutionClassId::bun_child_process())
        .with_restart_policy(RestartPolicy::on_failure(
            2,
            Duration::from_secs(5),
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        ))
        .with_capability(endpoint);
    let consumer = ModuleInstancePlan::new("bun-consumer", "fixture.bun.consumer")
        .with_entrypoint(script.to_string_lossy())
        .with_execution_class(ExecutionClassId::bun_child_process())
        .with_requirement(CapabilityRequirementPlan::one(
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
        ));
    AppComposition::new(
        vec![provider, consumer],
        vec![CapabilityBinding::new(
            "bun-consumer",
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            "bun-provider",
        )],
    )
    .resolve()
    .expect("Bun auth conformance plan should resolve")
}

fn run_actor_greeting(
    wire: BunWire,
    subject: &str,
    validity: Validity,
) -> Result<Result<GreetResponse, GreetError>, RuntimeFailure> {
    let driver = DeterministicDriver::new();
    let adapter = BunAdapter::new(bun_binary(), wire).with_codec(GreetingCodec);
    let app = driver.run(Kernel::start(
        greeting_plan(&fixture("request-provider.ts")),
        driver.clone(),
        ExecutionAdapterCatalog::single(adapter),
    ))?;
    let issuer = ActorAssertionIssuer::new("auth.users", b"shared-auth-key");
    let assertion = issuer.issue(
        subject,
        "user",
        "strong",
        [audience(CAPABILITY_ID, "greet")],
        validity,
        std::collections::BTreeMap::new(),
    );
    let context = assertion
        .attach(app.invocation_context(None, CancellationToken::new()))
        .expect("the authenticated context should accept one sealed assertion");
    let result = driver.run(app.invoke_with_context::<Greeting>(
        "bun-consumer",
        "greet",
        context,
        GreetRequest {
            name: "__requires_actor__".to_owned(),
        },
    ));
    let _ = driver.run(app.shutdown(Duration::from_secs(2)));
    result
}

fn run_authenticated_greeting(
    wire: BunWire,
    subject: &str,
) -> Result<Result<GreetResponse, GreetError>, RuntimeFailure> {
    run_actor_greeting(wire, subject, Validity::new(0, u64::MAX))
}

fn run_without_identity(
    wire: BunWire,
) -> Result<Result<GreetResponse, GreetError>, RuntimeFailure> {
    let driver = DeterministicDriver::new();
    let adapter = BunAdapter::new(bun_binary(), wire).with_codec(GreetingCodec);
    let app = driver.run(Kernel::start(
        greeting_plan(&fixture("request-provider.ts")),
        driver.clone(),
        ExecutionAdapterCatalog::single(adapter),
    ))?;
    let result = driver.run(app.invoke::<Greeting>(
        "bun-consumer",
        "greet",
        GreetRequest {
            name: "__requires_actor__".to_owned(),
        },
    ));
    let _ = driver.run(app.shutdown(Duration::from_secs(2)));
    result
}

#[test]
#[ignore = "requires Bun; CI runs ignored cross-runtime tests after installing Bun"]
fn both_wires_preserve_the_sealed_actor_assertion_for_target_authorization() {
    for wire in [BunWire::FramedStdio, BunWire::JsonRpcHttp] {
        let result = run_authenticated_greeting(wire, "user-123")
            .expect("authenticated call should reach the Bun target");
        assert_eq!(
            result.expect("authenticated target should not return a Domain Error"),
            GreetResponse {
                message: "Hello from Bun, user-123!".to_owned()
            },
        );
    }
}

#[test]
#[ignore = "requires Bun; CI runs ignored cross-runtime tests after installing Bun"]
fn target_business_authorization_remains_a_typed_domain_outcome() {
    for wire in [BunWire::FramedStdio, BunWire::JsonRpcHttp] {
        let result = run_authenticated_greeting(wire, "forbidden")
            .expect("authorization denial should reach the target");
        assert!(matches!(
            result,
            Err(GreetError::Unknown(ref error)) if error.code == "not_allowed"
        ));
    }
}

#[test]
#[ignore = "requires Bun; CI runs ignored cross-runtime tests after installing Bun"]
fn target_rejects_expired_actor_assertions_without_anonymous_fallback() {
    for wire in [BunWire::FramedStdio, BunWire::JsonRpcHttp] {
        let result = run_actor_greeting(wire, "user-123", Validity::new(0, 1))
            .expect("expired assertion should still reach the Bun target");
        assert!(matches!(
            result,
            Err(GreetError::Unknown(ref error)) if error.code == "actor_required"
        ));
    }
}

#[test]
#[ignore = "requires Bun; CI runs ignored cross-runtime tests after installing Bun"]
fn background_invocation_has_no_ambient_actor_identity() {
    for wire in [BunWire::FramedStdio, BunWire::JsonRpcHttp] {
        let result = run_without_identity(wire).expect("background call should reach the target");
        assert!(matches!(
            result,
            Err(GreetError::Unknown(ref error)) if error.code == "actor_required"
        ));
    }
}
