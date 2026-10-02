use std::{ptr, time::Duration};

use lenso_test::{
    ReceiptDifference, ScenarioReceiptEvent, ScenarioTerminal, ScenarioTransition, SimulatorFault,
    TestSimulator, first_receipt_difference,
};

fn event() -> ScenarioReceiptEvent {
    ScenarioReceiptEvent {
        virtual_time: Duration::from_nanos(17),
        generation_id: "generation-1".to_owned(),
        operation_id: "job-1".to_owned(),
        transition: ScenarioTransition::Started,
        fault: None,
        terminal: None,
    }
}

#[test]
fn empty_and_equal_receipts_have_no_difference() {
    assert!(first_receipt_difference(&[], &[]).is_none());
    let expected = [event(), event()];
    let actual = expected.clone();
    assert!(first_receipt_difference(&expected, &actual).is_none());
}

#[test]
fn first_changed_event_precedes_a_later_length_difference() {
    let expected = [event(), event()];
    let mut actual = vec![event(), event(), event()];
    actual[1].transition = ScenarioTransition::Paused;
    actual[2].operation_id = "later-event-must-not-appear".to_owned();

    let difference = first_receipt_difference(&expected, &actual).unwrap();
    assert_eq!(difference.index, 1);
    assert!(ptr::eq(
        difference.expected.unwrap(),
        &raw const expected[1]
    ));
    assert!(ptr::eq(difference.actual.unwrap(), &raw const actual[1]));
    for output in [format!("{difference}"), format!("{difference:?}")] {
        assert!(!output.contains("later-event-must-not-appear"));
        for detail in ["17", "generation-1", "job-1", "Started", "Paused"] {
            assert!(output.contains(detail), "missing {detail}: {output}");
        }
    }
}

#[test]
fn every_event_field_participates_in_full_equality() {
    let expected = [event()];
    let mut changed_time = event();
    changed_time.virtual_time += Duration::from_nanos(1);
    let mut changed_generation = event();
    changed_generation.generation_id.push('2');
    let mut changed_operation = event();
    changed_operation.operation_id.push('2');
    let mut changed_transition = event();
    changed_transition.transition = ScenarioTransition::DurableCommit;
    let mut changed_fault = event();
    changed_fault.fault = Some(SimulatorFault::ResourceUnavailable);
    let mut changed_terminal = event();
    changed_terminal.terminal = Some(ScenarioTerminal::Uncertain);

    for changed in [
        changed_time,
        changed_generation,
        changed_operation,
        changed_transition,
        changed_fault,
        changed_terminal,
    ] {
        let actual = [changed];
        let difference = first_receipt_difference(&expected, &actual).unwrap();
        assert_eq!(difference.index, 0);
        assert!(ptr::eq(
            difference.expected.unwrap(),
            &raw const expected[0]
        ));
        assert!(ptr::eq(difference.actual.unwrap(), &raw const actual[0]));
    }
}

#[test]
fn a_missing_suffix_is_reported_in_both_directions_including_empty() {
    let events = [event(), event(), event()];
    for prefix_length in [0, 2] {
        let prefix = &events[..prefix_length];
        let added = first_receipt_difference(prefix, &events).unwrap();
        assert_eq!(added.index, prefix_length);
        assert!(added.expected.is_none());
        assert!(ptr::eq(
            added.actual.unwrap(),
            &raw const events[prefix_length]
        ));

        let removed = first_receipt_difference(&events, prefix).unwrap();
        assert_eq!(removed.index, prefix_length);
        assert!(ptr::eq(
            removed.expected.unwrap(),
            &raw const events[prefix_length]
        ));
        assert!(removed.actual.is_none());
    }
}

#[test]
fn differences_beyond_display_truncation_preserve_the_complete_borrowed_events() {
    for change_generation in [true, false] {
        let mut before = event();
        before.generation_id = "g".repeat(4096);
        before.operation_id = "o".repeat(4096);
        let mut after = before.clone();
        if change_generation {
            before.generation_id.push_str("expected-tail");
            after.generation_id.push_str("actual-tail");
        } else {
            before.operation_id.push_str("expected-tail");
            after.operation_id.push_str("actual-tail");
        }
        let expected = [before];
        let actual = [after];
        let difference = first_receipt_difference(&expected, &actual).unwrap();
        assert_eq!(difference.index, 0);
        assert!(ptr::eq(
            difference.expected.unwrap(),
            &raw const expected[0]
        ));
        assert!(ptr::eq(difference.actual.unwrap(), &raw const actual[0]));
        assert_ne!(difference.expected, difference.actual);
        for output in [format!("{difference}"), format!("{difference:?}")] {
            assert!(output.len() < 1024);
            assert!(output.contains("..."));
            assert!(!output.contains("expected-tail"));
            assert!(!output.contains("actual-tail"));
            assert!(!output.contains(&"g".repeat(129)));
            assert!(!output.contains(&"o".repeat(129)));
        }
    }
}

#[test]
fn display_and_debug_escape_and_bound_long_unicode_and_control_characters() {
    // Public receipt event fields can be constructed without the recorder's validator.
    let hostile = "\n\r\t\u{1b}\u{7}\0\\\"非😼".repeat(4096);
    let mut before = event();
    before.virtual_time = Duration::MAX;
    before.generation_id = hostile.clone();
    before.operation_id = hostile.clone();
    before.transition = ScenarioTransition::ResponseStarted;
    before.fault = Some(SimulatorFault::DroppedConnection);
    before.terminal = Some(ScenarioTerminal::Uncertain);
    let mut after = before.clone();
    after.operation_id.push('x');
    let expected = [before];
    let actual = [after];
    let difference = ReceiptDifference {
        index: usize::MAX,
        ..first_receipt_difference(&expected, &actual).unwrap()
    };
    for output in [
        format!("{difference}"),
        format!("{difference:?}"),
        format!("{difference:#?}"),
    ] {
        assert!(
            output.len() < 1024,
            "formatted diagnostic must stay bounded"
        );
        assert!(output.is_ascii());
        assert!(
            output
                .bytes()
                .all(|byte| byte.is_ascii_graphic() || matches!(byte, b' ' | b'\n'))
        );
        assert_eq!(output.bytes().filter(|byte| *byte == b'\n').count(), 2);
        assert!(output.contains(&usize::MAX.to_string()));
        assert!(output.contains("..."));
        assert!(output.contains("\\n"));
        for detail in ["ResponseStarted", "DroppedConnection", "Uncertain"] {
            assert!(output.contains(detail), "missing {detail}: {output}");
        }
    }
    assert_eq!(difference.expected.unwrap().generation_id, hostile);
    assert!(ptr::eq(difference.actual.unwrap(), &raw const actual[0]));
}

#[test]
fn identifier_truncation_begins_only_after_128_escaped_ascii_bytes() {
    for length in [128, 129] {
        let mut before = event();
        before.generation_id = "g".repeat(length);
        before.operation_id = "o".repeat(length);
        let mut after = before.clone();
        after.transition = ScenarioTransition::Paused;
        let expected = [before];
        let actual = [after];
        let difference = first_receipt_difference(&expected, &actual).unwrap();
        for output in [format!("{difference}"), format!("{difference:?}")] {
            assert_eq!(output.contains("..."), length == 129);
            let suffix = if length == 129 { "..." } else { "" };
            assert!(output.contains(&format!("generation=\"{}{suffix}\"", "g".repeat(128))));
            assert!(output.contains(&format!("operation=\"{}{suffix}\"", "o".repeat(128))));
        }
    }
}

#[test]
fn truncation_keeps_unicode_and_control_escape_sequences_whole() {
    for suffix in ["非", "\n", "😼"] {
        let mut before = event();
        before.generation_id = format!("{}{suffix}", "a".repeat(127));
        // The two-byte newline escape exactly fills the other identifier budget.
        before.operation_id = format!("{}\n", "b".repeat(126));
        let mut after = before.clone();
        after.transition = ScenarioTransition::Paused;
        let expected = [before];
        let actual = [after];
        let difference = first_receipt_difference(&expected, &actual).unwrap();
        for output in [format!("{difference}"), format!("{difference:?}")] {
            assert!(output.contains(&format!("generation=\"{}...\"", "a".repeat(127))));
            assert!(output.contains(&format!("operation=\"{}\\n\"", "b".repeat(126))));
            assert_eq!(output.bytes().filter(|byte| *byte == b'\n').count(), 2);
            assert!(output.is_ascii());
        }
    }
}

#[test]
fn real_simulator_receipts_expose_timestamp_fault_and_terminal_boundaries() {
    let simulator = TestSimulator::new();
    let receipt = simulator.receipt();
    receipt
        .transition("generation-1", "job-1", ScenarioTransition::Started)
        .unwrap();
    let started = receipt.events();
    simulator.advance(Duration::from_millis(20));
    receipt
        .fault(
            "generation-1",
            "job-1",
            ScenarioTransition::Paused,
            SimulatorFault::ResourceUnavailable,
        )
        .unwrap();
    let faulted = receipt.events();
    let difference = first_receipt_difference(&started, &faulted).unwrap();
    assert_eq!(difference.index, 1);
    assert!(difference.expected.is_none());
    let fault = difference.actual.unwrap();
    assert_eq!(fault.virtual_time, Duration::from_millis(20));
    assert_eq!(fault.fault, Some(SimulatorFault::ResourceUnavailable));

    simulator.advance(Duration::from_millis(100));
    receipt
        .terminal("generation-1", "job-1", ScenarioTerminal::Failed)
        .unwrap();
    let finished = receipt.events();
    let difference = first_receipt_difference(&faulted, &finished).unwrap();
    assert_eq!(difference.index, 2);
    let terminal = difference.actual.unwrap();
    assert_eq!(terminal.virtual_time, Duration::from_millis(120));
    assert_eq!(terminal.terminal, Some(ScenarioTerminal::Failed));
    let output = format!("{difference}");
    assert!(output.contains("120000000"));
    assert!(output.contains("Failed"));
    assert!(first_receipt_difference(&finished, &receipt.events()).is_none());
}
