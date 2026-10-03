#[path = "store-contract/external.rs"]
mod external;
#[path = "store-contract/scenario.rs"]
mod scenario;

use external::{ExternalStore, Transcript};
use lenso_test::first_receipt_difference;

#[test]
fn replay_provider_store_corpus() {
    for provider in ["postgres", "d1"] {
        scenario::run(ExternalStore::replay(&Transcript::read(provider)), 31);
    }
}

#[test]
fn recorded_store_completions_replay_with_real_kernel_and_plugin() {
    for provider in ["postgres", "d1"] {
        let transcript = Transcript::read(provider);
        let first = scenario::run(ExternalStore::replay(&transcript), 31);
        let second = scenario::run(ExternalStore::replay(&transcript), 31);
        assert_eq!(first.0, second.0);
        assert_eq!(first_receipt_difference(&first.1, &second.1), None);
        // Four seeds, six cases each: an explicit finite exploration budget.
        // Timing varies; allowed database outcomes do not become a unique trace.
        for seed in 32..=34 {
            assert_eq!(
                scenario::run(ExternalStore::replay(&transcript), seed).0,
                first.0
            );
        }
    }
}

#[test]
#[ignore = "requires the real provider fixture; mandatory store-contract CI gate"]
fn real_provider_store_corpus() {
    let provider =
        std::env::var("LENSO_STORE_PROVIDER").expect("explicit real provider is required");
    scenario::run(ExternalStore::real(&provider), 31);
}

#[test]
#[should_panic(expected = "first external completion divergence at index 0")]
fn replay_rejects_a_changed_external_request() {
    let mut transcript = Transcript::read("postgres");
    transcript.completions[0].request = serde_json::json!({"action": "different"});
    scenario::run(ExternalStore::replay(&transcript), 31);
}
