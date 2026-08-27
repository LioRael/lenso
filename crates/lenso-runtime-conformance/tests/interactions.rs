use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan,
};
use lenso_kernel::{
    DeterministicDriver, EventAdmission, Kernel, NativeApp, NativeEventHandle, NativeStreamHandle,
    NoopPluginLifecycle, RuntimeDriver, RuntimeFailure, StreamEvent,
};
use lenso_runtime_conformance::{
    ConformanceExecutionAdapter, ConformancePlugin, ConformancePluginFactory,
    EVENT_PROBE_CAPABILITY_ID, EVENT_PROBE_CONSUMER_PACKAGE_ID, EVENT_PROBE_DESCRIPTOR_VERSION,
    EVENT_PROBE_OPERATION, EVENT_PROBE_PROVIDER_PACKAGE_ID, EventProbe, EventProbeProviderFactory,
    EventProbeRecorder, EventProbeValue, STREAM_PROBE_CAPABILITY_ID,
    STREAM_PROBE_CONSUMER_PACKAGE_ID, STREAM_PROBE_DESCRIPTOR_VERSION, STREAM_PROBE_OPERATION,
    STREAM_PROBE_PROVIDER_PACKAGE_ID, StreamProbe, StreamProbeError, StreamProbeMessage,
    StreamProbeOpen, StreamProbeProviderFactory,
};

#[derive(Debug)]
struct EmptyFactory {
    package_id: &'static str,
}

impl ConformancePluginFactory for EmptyFactory {
    fn package_id(&self) -> &'static str {
        self.package_id
    }

    fn instantiate(
        &self,
        _instance: &PluginInstancePlan,
    ) -> Result<ConformancePlugin, RuntimeFailure> {
        Ok(ConformancePlugin::with_lifecycle(
            Vec::new(),
            NoopPluginLifecycle,
        ))
    }
}

fn interaction_plan() -> lenso_app_plan::ResolvedAppPlan {
    AppComposition::new(
        vec![
            PluginInstancePlan::new("stream-consumer", STREAM_PROBE_CONSUMER_PACKAGE_ID)
                .with_requirement(CapabilityRequirementPlan::one(
                    STREAM_PROBE_CAPABILITY_ID,
                    STREAM_PROBE_DESCRIPTOR_VERSION,
                )),
            PluginInstancePlan::new("stream-provider", STREAM_PROBE_PROVIDER_PACKAGE_ID)
                .with_capability(
                    CapabilityEndpointPlan::new(
                        STREAM_PROBE_CAPABILITY_ID,
                        STREAM_PROBE_DESCRIPTOR_VERSION,
                        [STREAM_PROBE_OPERATION],
                    )
                    .with_stream_operation(STREAM_PROBE_OPERATION)
                    .with_limits(0, 1),
                ),
            PluginInstancePlan::new("event-consumer", EVENT_PROBE_CONSUMER_PACKAGE_ID)
                .with_requirement(CapabilityRequirementPlan::many(
                    EVENT_PROBE_CAPABILITY_ID,
                    EVENT_PROBE_DESCRIPTOR_VERSION,
                )),
            PluginInstancePlan::new("event-provider-a", EVENT_PROBE_PROVIDER_PACKAGE_ID)
                .with_capability(
                    CapabilityEndpointPlan::new(
                        EVENT_PROBE_CAPABILITY_ID,
                        EVENT_PROBE_DESCRIPTOR_VERSION,
                        [EVENT_PROBE_OPERATION],
                    )
                    .with_event_operation(EVENT_PROBE_OPERATION)
                    .with_event_capacity(2),
                ),
            PluginInstancePlan::new("event-provider-b", EVENT_PROBE_PROVIDER_PACKAGE_ID)
                .with_capability(
                    CapabilityEndpointPlan::new(
                        EVENT_PROBE_CAPABILITY_ID,
                        EVENT_PROBE_DESCRIPTOR_VERSION,
                        [EVENT_PROBE_OPERATION],
                    )
                    .with_event_operation(EVENT_PROBE_OPERATION)
                    .with_event_capacity(2),
                ),
        ],
        vec![
            CapabilityBinding::new(
                "stream-consumer",
                STREAM_PROBE_CAPABILITY_ID,
                STREAM_PROBE_DESCRIPTOR_VERSION,
                "stream-provider",
            ),
            CapabilityBinding::new(
                "event-consumer",
                EVENT_PROBE_CAPABILITY_ID,
                EVENT_PROBE_DESCRIPTOR_VERSION,
                "event-provider-a",
            ),
            CapabilityBinding::new(
                "event-consumer",
                EVENT_PROBE_CAPABILITY_ID,
                EVENT_PROBE_DESCRIPTOR_VERSION,
                "event-provider-b",
            ),
        ],
    )
    .resolve()
    .expect("the interaction-complete conformance Plan should resolve")
}

fn exercise_stream(
    driver: &DeterministicDriver,
    app: &NativeApp,
) -> NativeStreamHandle<StreamProbe> {
    let stream_handle = app
        .stream_handle::<StreamProbe>("stream-consumer")
        .expect("the stream conformance binding should resolve");
    let stream = driver
        .run(stream_handle.open(
            STREAM_PROBE_OPERATION,
            StreamProbeOpen {
                value: "session".to_owned(),
            },
        ))
        .expect("stream open should not fail")
        .expect("stream open should not return a Domain Error");
    driver
        .run(stream.send(StreamProbeMessage {
            sequence: 1,
            value: "value".to_owned(),
        }))
        .expect("stream send should succeed");
    assert_eq!(
        driver.run(stream.receive()),
        Ok(StreamEvent::Message(StreamProbeMessage {
            sequence: 1,
            value: "session: value".to_owned(),
        }))
    );
    driver
        .run(stream.close_send())
        .expect("stream half-close should succeed");
    assert_eq!(
        driver.run(stream.receive()),
        Ok(StreamEvent::PeerHalfClosed)
    );
    assert_eq!(
        driver.run(stream.receive()),
        Ok(StreamEvent::Terminal(Ok(())))
    );
    drop(stream);
    assert!(matches!(
        driver
            .run(stream_handle.open(
                STREAM_PROBE_OPERATION,
                StreamProbeOpen {
                    value: "reject".to_owned(),
                },
            ))
            .expect("a Domain Error is not a Runtime Failure"),
        Err(StreamProbeError::Rejected)
    ));
    stream_handle
}

fn exercise_event(
    driver: &DeterministicDriver,
    app: &NativeApp,
    recorder: &EventProbeRecorder,
) -> NativeEventHandle<EventProbe> {
    let event_handle = app
        .many_event_handle::<EventProbe>("event-consumer")
        .expect("the Event conformance bindings should resolve");
    let admissions = driver.run(event_handle.publish(
        EVENT_PROBE_OPERATION,
        EventProbeValue {
            sequence: 1,
            value: "value".to_owned(),
        },
    ));
    assert_eq!(
        admissions
            .iter()
            .map(|result| (result.subscriber_instance(), result.admission()))
            .collect::<Vec<_>>(),
        vec![
            ("event-provider-a", EventAdmission::Accepted),
            ("event-provider-b", EventAdmission::Accepted),
        ]
    );
    driver.run(async { driver.yield_now().await });
    assert_eq!(
        recorder.values(),
        vec![
            EventProbeValue {
                sequence: 1,
                value: "value".to_owned(),
            },
            EventProbeValue {
                sequence: 1,
                value: "value".to_owned(),
            },
        ]
    );
    event_handle
}

#[test]
fn conformance_adapter_exercises_stream_event_and_shutdown_through_one_interface() {
    let driver = DeterministicDriver::new();
    let recorder = EventProbeRecorder::default();
    let adapter = ConformanceExecutionAdapter::new()
        .with_factory(EmptyFactory {
            package_id: STREAM_PROBE_CONSUMER_PACKAGE_ID,
        })
        .with_factory(StreamProbeProviderFactory)
        .with_factory(EmptyFactory {
            package_id: EVENT_PROBE_CONSUMER_PACKAGE_ID,
        })
        .with_factory(EventProbeProviderFactory::new(recorder.clone()));
    let app = driver
        .run(Kernel::start_native(
            interaction_plan(),
            driver.clone(),
            adapter,
        ))
        .expect("the interaction-complete conformance App should start");

    let stream_handle = exercise_stream(&driver, &app);
    let event_handle = exercise_event(&driver, &app, &recorder);

    assert_eq!(
        driver.run(app.shutdown(std::time::Duration::from_secs(1))),
        lenso_kernel::ShutdownOutcome::Clean
    );
    assert!(matches!(
        driver.run(stream_handle.open(
            STREAM_PROBE_OPERATION,
            StreamProbeOpen {
                value: "late".to_owned(),
            },
        )),
        Err(RuntimeFailure::AdmissionClosed)
    ));
    let late_admissions = driver.run(event_handle.publish(
        EVENT_PROBE_OPERATION,
        EventProbeValue {
            sequence: 2,
            value: "late".to_owned(),
        },
    ));
    assert!(
        late_admissions
            .iter()
            .all(|result| result.admission() == EventAdmission::Unavailable)
    );
}
