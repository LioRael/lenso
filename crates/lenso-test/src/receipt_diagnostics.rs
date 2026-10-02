//! Locate the first difference between two observed scenario receipts.
//!
//! This compares recorded evidence; it does not replay a scenario, inject a
//! fault, advance time, or infer which execution produced the correct result.

use std::fmt;

use crate::ScenarioReceiptEvent;

/// The first unequal event, or the first event missing from one receipt.
///
/// The events are borrowed in full, including identifiers beyond the diagnostic
/// display limit. `Display` and `Debug` show only this position, in expected /
/// actual order, with virtual time in nanoseconds and the finite event enums.
/// Each identifier is escaped and limited to 128 escaped ASCII bytes, followed
/// by `...` if truncated. Both formats produce fewer than 1024 bytes regardless
/// of receipt length or identifier length, including directly constructed events
/// that bypass the recorder's identifier validation. This is output bounding,
/// not secret redaction; use only test-owned, non-sensitive identifiers.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ReceiptDifference<'a> {
    /// Zero-based index of the first difference.
    pub index: usize,
    /// Expected event at this position, or `None` if the expected receipt ended.
    pub expected: Option<&'a ScenarioReceiptEvent>,
    /// Actual event at this position, or `None` if the actual receipt ended.
    pub actual: Option<&'a ScenarioReceiptEvent>,
}

/// Returns the first event difference, or `None` when both receipts are equal.
///
/// Compares every field using the existing event equality semantics. A shared
/// prefix followed by an extra event differs at the prefix's length. Comparison
/// allocates no copy of either receipt and never truncates identifiers; only
/// the returned diagnostic's formatting has an output limit.
///
/// ```
/// use std::time::Duration;
/// use lenso_test::{TestSimulator, ScenarioTransition, first_receipt_difference};
///
/// let simulator = TestSimulator::new();
/// let expected = simulator.receipt();
/// expected.transition("generation-1", "job-1", ScenarioTransition::Started)?;
/// simulator.advance(Duration::from_millis(1));
/// let actual = simulator.receipt();
/// actual.transition("generation-1", "job-1", ScenarioTransition::Started)?;
/// let expected_events = expected.events();
/// let actual_events = actual.events();
/// let difference = first_receipt_difference(&expected_events, &actual_events).unwrap();
/// assert_eq!(difference.index, 0);
/// assert_eq!(difference.actual.unwrap().virtual_time, Duration::from_millis(1));
/// assert!(difference.to_string().contains("time_ns=1000000"));
/// # Ok::<(), lenso_test::FaultPointError>(())
/// ```
pub fn first_receipt_difference<'a>(
    expected: &'a [ScenarioReceiptEvent],
    actual: &'a [ScenarioReceiptEvent],
) -> Option<ReceiptDifference<'a>> {
    let index = expected
        .iter()
        .zip(actual)
        .position(|(expected, actual)| expected != actual)
        .or_else(|| (expected.len() != actual.len()).then_some(expected.len().min(actual.len())))?;
    Some(ReceiptDifference {
        index,
        expected: expected.get(index),
        actual: actual.get(index),
    })
}

impl fmt::Display for ReceiptDifference<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "receipt differs at event {}\nexpected: ",
            self.index
        )?;
        write_event(formatter, self.expected)?;
        formatter.write_str("\nactual: ")?;
        write_event(formatter, self.actual)
    }
}

// Assertion failures use Debug. Keep it bounded just like the explicit display.
impl fmt::Debug for ReceiptDifference<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

fn write_event(
    formatter: &mut fmt::Formatter<'_>,
    event: Option<&ScenarioReceiptEvent>,
) -> fmt::Result {
    let Some(event) = event else {
        return formatter.write_str("<end of receipt>");
    };
    write!(
        formatter,
        "time_ns={} generation=",
        event.virtual_time.as_nanos()
    )?;
    write_identifier(formatter, &event.generation_id)?;
    formatter.write_str(" operation=")?;
    write_identifier(formatter, &event.operation_id)?;
    write!(
        formatter,
        " transition={:?} fault={:?} terminal={:?}",
        event.transition, event.fault, event.terminal,
    )
}

fn write_identifier(formatter: &mut fmt::Formatter<'_>, identifier: &str) -> fmt::Result {
    let mut remaining = 128;
    formatter.write_str("\"")?;
    for character in identifier.chars() {
        let escaped_size = character.escape_default().count();
        if escaped_size > remaining {
            formatter.write_str("...")?;
            break;
        }
        write!(formatter, "{}", character.escape_default())?;
        remaining -= escaped_size;
    }
    formatter.write_str("\"")
}
