//! The same real Kernel and Plugin fixture for live IO and completion replay.

use std::{cell::RefCell, rc::Rc, time::Duration};

use lenso_app_plan::{PluginInstancePlan, ResolvedAppPlan};
use lenso_kernel::{
    ActivateContext, PluginFuture, PluginLifecycle, RuntimeFailure, ShutdownOutcome,
};
use lenso_native_adapter::{NativePluginFactory, NativePluginFactoryContext, NativePluginInstance};
use lenso_test::{
    CommitKnowledge, DurableFailure, DurableFailureCause, DurableFaultFacade, ScenarioBoundary,
    ScenarioReceiptEvent, ScenarioTerminal, ScenarioTransition, SimulatorFault, TestApp,
    TestEntropy, TestSimulator,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::external::ExternalStore;

#[derive(Clone, Debug, Deserialize)]
pub struct Case {
    pub id: String,
    pub amount: u64,
    pub rollback: bool,
    pub fault: Option<String>,
    pub knowledge: String,
    pub persisted: bool,
    pub completion_ms: u64,
}

#[derive(Deserialize)]
pub struct Corpus {
    pub version: u8,
    pub seed: u8,
    pub cases: Vec<Case>,
    pub race: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    pub knowledge: CommitKnowledge,
    pub acknowledged: bool,
    pub fault: Option<SimulatorFault>,
    pub state_before_retry: Value,
    pub state_after_retry: Value,
}

#[derive(Clone, Debug)]
struct StoreFixture {
    store: Rc<RefCell<ExternalStore>>,
    simulator: TestSimulator,
    case: Case,
    delay_ms: u64,
    result: Rc<RefCell<Option<Observation>>>,
}

impl PluginLifecycle for StoreFixture {
    fn activate(&self, context: ActivateContext) -> PluginFuture {
        let fixture = self.clone();
        let spawned = context.tasks().spawn_local(Box::pin(async move {
            let facade =
                DurableFaultFacade::new("store.atomic", fixture.simulator.faults()).unwrap();
            let outcome = facade
                .execute(|| async {
                    let response = fixture.store.borrow_mut().call(apply(
                        &fixture.case,
                        fixture.case.amount,
                        fixture.case.rollback,
                    ));
                    // The external completion is published at a controlled virtual
                    // instant. Only this IO boundary is replayed; the same Plugin
                    // and Kernel code above and below it runs in both modes.
                    let deadline =
                        fixture.simulator.now() + Duration::from_millis(fixture.delay_ms);
                    fixture.simulator.sleep_until(deadline).await;
                    decode(&response)
                })
                .await;
            let acknowledged = outcome.is_ok();
            assert_eq!(
                acknowledged,
                fixture.case.fault.is_none() && !fixture.case.rollback
            );
            let (knowledge, fault) = match outcome {
                Ok(value) => {
                    assert_eq!(
                        value,
                        json!({"id": fixture.case.id, "amount": fixture.case.amount})
                    );
                    (CommitKnowledge::Committed, None)
                }
                Err(error) => {
                    let fault = match error.cause {
                        DurableFailureCause::Injected(fault) => Some(fault),
                        DurableFailureCause::Backend(error) => {
                            assert_eq!(error, "constraint");
                            None
                        }
                    };
                    assert_eq!(
                        fault,
                        fixture.case.fault.as_ref().map(|_| SimulatorFault::Timeout)
                    );
                    (error.knowledge, fault)
                }
            };
            assert_eq!(knowledge_name(knowledge), fixture.case.knowledge);
            let before = fixture
                .store
                .borrow_mut()
                .call(json!({"action": "read", "id": fixture.case.id}));
            assert_state(
                &before,
                &fixture.case.id,
                fixture.case.persisted.then_some(fixture.case.amount),
            );
            // Reconcile/retry through the SAME idempotent finite operation,
            // including unknown outcomes. A changed payload cannot overwrite an
            // already committed result. No timeout grants a blind second effect.
            let retry_amount = fixture.case.amount + 100;
            let retry = fixture
                .store
                .borrow_mut()
                .call(apply(&fixture.case, retry_amount, false));
            let expected = if fixture.case.persisted {
                fixture.case.amount
            } else {
                retry_amount
            };
            assert_eq!(
                decode(&retry).unwrap(),
                json!({"id": fixture.case.id, "amount": expected})
            );
            let after = fixture
                .store
                .borrow_mut()
                .call(json!({"action": "read", "id": fixture.case.id}));
            assert_state(&after, &fixture.case.id, Some(expected));
            *fixture.result.borrow_mut() = Some(Observation {
                knowledge,
                acknowledged,
                fault,
                state_before_retry: before,
                state_after_retry: after,
            });
        }));
        Box::pin(async move {
            spawned
                .map(|_| ())
                .map_err(|error| RuntimeFailure::PluginFailure {
                    detail: format!("store fixture spawn: {error:?}"),
                })
        })
    }
}

// The factory and lifecycle are test-owned contract fixtures, not a production
// product Store adapter. No production business implementation is duplicated.
impl NativePluginFactory for StoreFixture {
    fn package_id(&self) -> &'static str {
        "test.durable-store-contract"
    }

    fn instantiate(
        &self,
        _context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::with_lifecycle(
            Vec::new(),
            self.clone(),
        ))
    }
}

pub fn run(store: ExternalStore, seed: u8) -> (Vec<Observation>, Vec<ScenarioReceiptEvent>) {
    let corpus: Corpus = serde_json::from_str(include_str!("corpus.json")).unwrap();
    assert_eq!(corpus.version, 1);
    assert_eq!(corpus.seed, 31);
    assert_eq!(corpus.cases.len(), 6, "bounded scenario budget");
    let store = Rc::new(RefCell::new(store));
    let version = store.borrow_mut().call(json!({"action": "version"}));
    assert!(
        version["version"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    let entropy = TestEntropy::seeded([seed; 32]);
    let mut observations = vec![];
    let mut events = vec![];
    for (index, case) in corpus.cases.iter().enumerate() {
        let simulator = TestSimulator::new();
        let receipt = simulator.receipt();
        let operation = format!("case-{index}");
        receipt
            .transition("store-fixture", &operation, ScenarioTransition::Started)
            .unwrap();
        if let Some(fault) = &case.fault {
            let boundary = match fault.as_str() {
                "before_operation" => ScenarioBoundary::BeforeOperation,
                "after_durable_commit" => ScenarioBoundary::AfterDurableCommit,
                "before_response" => ScenarioBoundary::BeforeResponse,
                _ => panic!("unknown corpus fault boundary"),
            };
            simulator
                .faults()
                .inject_at("store.atomic", boundary, SimulatorFault::Timeout)
                .unwrap();
        }
        let mut jitter = [0];
        entropy.fill(&mut jitter);
        let delay_ms = if case.completion_ms == 0 {
            0
        } else {
            case.completion_ms + u64::from(jitter[0] % 3)
        };
        let result = Rc::new(RefCell::new(None));
        let app = TestApp::builder(ResolvedAppPlan::new(
            vec![PluginInstancePlan::new(
                "store",
                "test.durable-store-contract",
            )],
            vec![],
        ))
        .with_simulator(simulator.clone())
        .with_factory(StoreFixture {
            store: store.clone(),
            simulator: simulator.clone(),
            case: case.clone(),
            delay_ms,
            result: result.clone(),
        })
        .start()
        .unwrap();
        simulator.pump();
        if delay_ms > 0 {
            assert!(result.borrow().is_none());
            simulator.advance(Duration::from_millis(delay_ms - 1));
            simulator.pump();
            assert!(
                result.borrow().is_none(),
                "external completion must not arrive early"
            );
            simulator.advance(Duration::from_millis(1));
            simulator.pump();
        }
        let observation = result
            .borrow_mut()
            .take()
            .expect("completion at exact controlled instant");
        let terminal = match observation.knowledge {
            CommitKnowledge::Unknown => ScenarioTerminal::Uncertain,
            CommitKnowledge::NotExecuted | CommitKnowledge::RolledBack => ScenarioTerminal::Failed,
            CommitKnowledge::Committed if !observation.acknowledged => ScenarioTerminal::Uncertain,
            CommitKnowledge::Committed => ScenarioTerminal::Succeeded,
        };
        if let Some(fault) = observation.fault {
            receipt
                .fault(
                    "store-fixture",
                    &operation,
                    ScenarioTransition::ResponseStarted,
                    fault,
                )
                .unwrap();
        }
        receipt
            .terminal("store-fixture", &operation, terminal)
            .unwrap();
        events.extend(receipt.events());
        observations.push(observation);
        assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
    }

    verify_race_and_restart(&store, &corpus, &observations);
    store.borrow().finish();
    (observations, events)
}

fn verify_race_and_restart(
    store: &Rc<RefCell<ExternalStore>>,
    corpus: &Corpus,
    observations: &[Observation],
) {
    // Real providers may choose either winner. Do not require a unique parallel
    // trace, only one complete atomic result shared by both callers.
    let id = corpus.race["id"].as_str().unwrap();
    let race = store
        .borrow_mut()
        .call(json!({"action": "race", "id": id, "amounts": corpus.race["amounts"]}));
    let replies = race["receipts"].as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0], replies[1]);
    assert_eq!(replies[0]["id"], id);
    let amount = replies[0]["amount"].as_u64().unwrap();
    assert!(
        corpus.race["amounts"]
            .as_array()
            .unwrap()
            .contains(&json!(amount))
    );
    assert_state(
        &store.borrow_mut().call(json!({"action": "read", "id": id})),
        id,
        Some(amount),
    );
    assert_eq!(
        store.borrow_mut().call(json!({"action": "restart"})),
        json!({"ready": true})
    );
    for (case, observation) in corpus.cases.iter().zip(observations) {
        let state = store
            .borrow_mut()
            .call(json!({"action": "read", "id": case.id}));
        assert_eq!(
            state, observation.state_after_retry,
            "durability after provider recreation"
        );
    }
    assert_state(
        &store.borrow_mut().call(json!({"action": "read", "id": id})),
        id,
        Some(amount),
    );
}

fn apply(case: &Case, amount: u64, rollback: bool) -> Value {
    json!({"action": "apply", "id": case.id, "amount": amount, "rollback": rollback})
}

fn decode(response: &Value) -> Result<Value, DurableFailure<String>> {
    match response["kind"].as_str().unwrap() {
        "committed" => Ok(response["value"].clone()),
        "rolled_back" => Err(DurableFailure::backend(
            CommitKnowledge::RolledBack,
            "constraint".to_owned(),
        )),
        "unknown" => Err(DurableFailure::backend(
            CommitKnowledge::Unknown,
            "backend".to_owned(),
        )),
        _ => panic!("unknown typed backend completion"),
    }
}

fn assert_state(state: &Value, id: &str, amount: Option<u64>) {
    let value = amount.map_or(Value::Null, |amount| json!({"id": id, "amount": amount}));
    assert_eq!(
        state,
        &json!({"receipt": value, "effect": value}),
        "atomic receipt/effect and bound values"
    );
}

fn knowledge_name(knowledge: CommitKnowledge) -> &'static str {
    match knowledge {
        CommitKnowledge::NotExecuted => "not_executed",
        CommitKnowledge::RolledBack => "rolled_back",
        CommitKnowledge::Committed => "committed",
        CommitKnowledge::Unknown => "unknown",
    }
}
