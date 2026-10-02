//! Admission rejection must not interrupt another turn on the same Engine.
use std::{future::Future, pin::Pin, task::Context};

use futures::{executor::block_on, task::noop_waker_ref};
use lenso_kernel::CancellationToken;

use super::*;

struct EpochProbe {
    store: Store<()>,
    function: wasmtime::TypedFunc<(), ()>,
}

impl EpochProbe {
    fn new(engine: &Engine) -> Self {
        // (module (func (export "probe") (loop (br 0))))
        // Fuel bounds the real Wasmtime probe even without an epoch interrupt.
        let module = wasmtime::Module::new(
            engine,
            [
                0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 9, 1, 5, 112, 114,
                111, 98, 101, 0, 0, 10, 9, 1, 7, 0, 3, 64, 12, 0, 11, 11,
            ],
        )
        .unwrap();
        let mut store = Store::new(engine, ());
        store.set_fuel(100).unwrap();
        store.set_epoch_deadline(1);
        store.epoch_deadline_trap();
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let function = instance.get_typed_func(&mut store, "probe").unwrap();
        Self { store, function }
    }

    fn assert_trap(&mut self, expected: wasmtime::Trap) {
        let error = self.function.call(&mut self.store, ()).unwrap_err();
        assert_eq!(error.downcast_ref::<wasmtime::Trap>(), Some(&expected));
    }
}

fn generation() -> (
    Rc<WasmGeneration>,
    mpsc::Receiver<WorkerCommand>,
    EpochProbe,
) {
    let mut config = Config::new();
    config.consume_fuel(true).epoch_interruption(true);
    let engine = Engine::new(&config).unwrap();
    let probe = EpochProbe::new(&engine);
    let (commands, receiver) = mpsc::sync_channel(1);
    let generation = Rc::new(WasmGeneration {
        commands,
        engine,
        failed: Arc::new(AtomicBool::new(false)),
        worker: std::cell::RefCell::new(None),
        stopped: std::cell::Cell::new(false),
        active_streams: std::cell::Cell::new(0),
        max_streams: 1,
        max_host_imports_per_call: 1,
        max_turn: Duration::from_secs(60),
        host_imports: Rc::new(JsonHostImports::new(vec![], 1).unwrap()),
    });
    (generation, receiver, probe)
}

fn context() -> InvocationContext {
    InvocationContext::new(1, None, CancellationToken::new())
}

fn request(
    generation: Rc<WasmGeneration>,
    context: InvocationContext,
) -> impl Future<Output = Result<JsonInvocationOutcome, RuntimeFailure>> + use<> {
    generation.invoke("test.echo@1".into(), "echo".into(), "null".into(), context)
}

fn assert_pending<F: Future + ?Sized>(future: Pin<&mut F>) {
    assert!(
        future
            .poll(&mut Context::from_waker(noop_waker_ref()))
            .is_pending()
    );
}

fn receive_command(receiver: &mpsc::Receiver<WorkerCommand>) -> GuestCommand {
    let WorkerCommand::Call(command) = receiver.try_recv().unwrap() else {
        panic!("expected an admitted call");
    };
    command
}

fn assert_admission_rejection<T>(result: Result<T, RuntimeFailure>, operation: &str) {
    assert!(matches!(
        result,
        Err(RuntimeFailure::ResourceExhausted {
            capability: EXECUTION_CLASS,
            operation: rejected_operation,
        }) if rejected_operation == operation
    ));
}

fn stream(generation: &Rc<WasmGeneration>) -> Rc<WasmStreamSession> {
    generation.reserve_stream().unwrap();
    Rc::new(WasmStreamSession {
        generation: generation.clone(),
        stream_id: 1,
        context: context(),
        cancelled: std::cell::Cell::new(false),
        finished: std::cell::Cell::new(false),
    })
}

#[test]
fn full_command_queue_does_not_advance_the_shared_engine_epoch() {
    let (generation, receiver, mut probe) = generation();
    // Occupy the slot with a real admitted call; keep its caller pending.
    let mut admitted = Box::pin(request(generation.clone(), context()));
    assert_pending(admitted.as_mut());

    assert_admission_rejection(block_on(request(generation.clone(), context())), "invoke");
    assert!(!generation.failed.load(Ordering::Acquire));
    let command = receive_command(&receiver);
    assert!(!command.abandoned.load(Ordering::Acquire));
    probe.assert_trap(wasmtime::Trap::OutOfFuel);

    command
        .outcome
        .send(Ok(JsonInvocationOutcome::Success(serde_json::Value::Null)))
        .unwrap();
    assert!(block_on(admitted).is_ok());
}

#[test]
fn disconnected_command_queue_does_not_advance_the_shared_engine_epoch() {
    let (generation, receiver, mut probe) = generation();
    drop(receiver);
    assert_admission_rejection(block_on(request(generation.clone(), context())), "invoke");
    assert!(!generation.failed.load(Ordering::Acquire));
    probe.assert_trap(wasmtime::Trap::OutOfFuel);
}

#[test]
fn rejected_stream_open_releases_reservation_without_interrupting() {
    let (generation, receiver, mut probe) = generation();
    let mut admitted = Box::pin(request(generation.clone(), context()));
    assert_pending(admitted.as_mut());

    assert_admission_rejection(
        block_on(generation.clone().open(
            "test.stream@1".into(),
            "chat".into(),
            "null".into(),
            context(),
        )),
        "stream-open",
    );
    assert_eq!(generation.active_streams.get(), 0);
    assert!(!generation.failed.load(Ordering::Acquire));
    probe.assert_trap(wasmtime::Trap::OutOfFuel);

    receive_command(&receiver)
        .outcome
        .send(Ok(JsonInvocationOutcome::Success(serde_json::Value::Null)))
        .unwrap();
    assert!(block_on(admitted).is_ok());
}

#[test]
fn disconnected_stream_receive_does_not_interrupt_and_releases_on_drop() {
    let (generation, receiver, mut probe) = generation();
    let session = stream(&generation);
    drop(receiver);

    assert_admission_rejection(block_on(session.clone().receive()), "stream-receive");
    assert!(!generation.failed.load(Ordering::Acquire));
    assert_eq!(generation.active_streams.get(), 1);
    drop(session);
    assert_eq!(generation.active_streams.get(), 0);
    probe.assert_trap(wasmtime::Trap::OutOfFuel);
}

#[test]
fn admitted_request_abandonment_still_interrupts() {
    let (generation, receiver, mut probe) = generation();
    let mut admitted = Box::pin(request(generation.clone(), context()));
    assert_pending(admitted.as_mut());
    let command = receive_command(&receiver);

    drop(admitted);

    assert!(command.abandoned.load(Ordering::Acquire));
    assert!(command.outcome.is_canceled());
    probe.assert_trap(wasmtime::Trap::Interrupt);
}

#[test]
fn admitted_stream_receive_abandonment_still_interrupts() {
    let (generation, receiver, mut probe) = generation();
    let session = stream(&generation);
    let mut admitted = session.clone().receive();
    assert_pending(admitted.as_mut());
    let command = receive_command(&receiver);
    assert!(matches!(
        command.call,
        GuestCall::StreamReceive { stream_id: 1 }
    ));

    drop(admitted);

    assert!(command.abandoned.load(Ordering::Acquire));
    assert!(command.outcome.is_canceled());
    drop(session);
    assert_eq!(generation.active_streams.get(), 0);
    probe.assert_trap(wasmtime::Trap::Interrupt);
}

#[test]
fn admitted_request_cancellation_still_interrupts_and_retires_generation() {
    let (generation, receiver, mut probe) = generation();
    let context = context();
    let cancellation = context.cancellation();
    let mut admitted = Box::pin(request(generation.clone(), context));
    assert_pending(admitted.as_mut());
    let command = receive_command(&receiver);

    cancellation.cancel();
    cancellation.cancel();
    assert!(matches!(
        block_on(admitted),
        Err(RuntimeFailure::Cancelled { request_id: 1 })
    ));
    assert!(generation.failed.load(Ordering::Acquire));
    assert!(command.abandoned.load(Ordering::Acquire));
    assert!(command.outcome.is_canceled());
    probe.assert_trap(wasmtime::Trap::Interrupt);
}

#[test]
fn completed_request_disarms_abandonment_guard() {
    let (generation, receiver, mut probe) = generation();
    let mut admitted = Box::pin(request(generation.clone(), context()));
    assert_pending(admitted.as_mut());
    let command = receive_command(&receiver);
    let abandoned = command.abandoned;
    command
        .outcome
        .send(Ok(JsonInvocationOutcome::Success(serde_json::Value::Null)))
        .unwrap();

    assert!(block_on(admitted).is_ok());
    assert!(!abandoned.load(Ordering::Acquire));
    assert!(!generation.failed.load(Ordering::Acquire));
    probe.assert_trap(wasmtime::Trap::OutOfFuel);
}
