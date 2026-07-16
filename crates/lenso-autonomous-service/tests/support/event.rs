use lenso_service::{EventEnvelope, GeneratedEventContract, validate_event_envelope};

pub(crate) fn support_ticket_opened(event_id: &str, ticket_id: &str) -> EventEnvelope {
    let mut envelope: EventEnvelope = serde_json::from_str(include_str!(
        "../../../../contracts/events/support/support.ticket-opened.v1.envelope.json"
    ))
    .unwrap();
    event_id.clone_into(&mut envelope.event_id);
    envelope.content.data["ticketId"] = serde_json::json!(ticket_id);
    let contract: GeneratedEventContract = serde_json::from_str(include_str!(
        "../../../../contracts/events/support/support.ticket-opened.v1.artifact.json"
    ))
    .unwrap();
    assert_eq!(validate_event_envelope(&contract, &envelope), vec![]);
    envelope
}
