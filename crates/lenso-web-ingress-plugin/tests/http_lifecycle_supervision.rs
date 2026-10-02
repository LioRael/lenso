//! Real loopback qualification of execution-owned failure supervision.
//! Request disconnection drops the HTTP caller; Stream disconnection cancels
//! its session. Neither requires the provider's pending work to stop promptly.

#[path = "support/http_lifecycle.rs"]
mod support;

use std::{net::SocketAddr, sync::atomic::Ordering, time::Duration};

use lenso_kernel::{DiagnosticEvent, DiagnosticFilter, DiagnosticSource, Kernel, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_runner::TokioDriver;
use lenso_web_ingress_plugin::WebIngressFactory;
use support::{DisconnectWitness, Factory, Kind, Probe, plan};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::LocalSet,
};

const BOUND: Duration = Duration::from_secs(3);

async fn wait_until(condition: impl Fn() -> bool) -> bool {
    tokio::time::timeout(BOUND, async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .is_ok()
}

async fn begin_request(address: SocketAddr) -> TcpStream {
    let mut connection = TcpStream::connect(address).await.unwrap();
    connection
        .write_all(b"GET /work HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
    connection
}

#[allow(
    clippy::too_many_lines,
    reason = "keep the real transport lifecycle and cleanup in one ordered scenario"
)]
async fn qualification(kind: Kind) {
    let (finish, gate) = tokio::sync::oneshot::channel();
    let probe = Probe::new(gate);
    let witness = DisconnectWitness::default();
    let ingress = WebIngressFactory::default().with_middleware(witness.clone());
    let app = Kernel::start_native(
        plan(kind),
        TokioDriver::new(),
        NativePluginRegistry::new()
            .with_factory(Factory {
                kind,
                probe: probe.clone(),
            })
            .with_factory(ingress.clone()),
    )
    .await
    .unwrap();
    let address = ingress.local_address().unwrap();
    assert!(address.ip().is_loopback());
    let observer = app
        .diagnostics()
        .subscribe(DiagnosticFilter::only(DiagnosticSource::Supervision), 16)
        .unwrap();
    eprintln!(
        "WEB_K1 start pid={} kind={kind:?} address={address} generation={:?}",
        std::process::id(),
        app.plugin_generation("provider")
    );
    assert_eq!(app.plugin_generation("provider"), Some(1));
    let mut connection = begin_request(address).await;
    if matches!(kind, Kind::Stream) {
        let mut wire = Vec::new();
        tokio::time::timeout(BOUND, async {
            while !wire
                .windows(b"first-chunk".len())
                .any(|part| part == b"first-chunk")
            {
                let mut bytes = [0_u8; 512];
                let count = connection.read(&mut bytes).await.unwrap();
                assert_ne!(count, 0, "stream closed before its first chunk");
                wire.extend_from_slice(&bytes[..count]);
                assert!(wire.len() < 4096);
            }
        })
        .await
        .expect("real HTTP response head and first chunk");
        assert!(wire.starts_with(b"HTTP/1.1 200"));
        eprintln!(
            "WEB_K1 first HTTP stream chunk received bytes={}",
            wire.len()
        );
    }
    assert!(
        wait_until(|| probe.started.get()).await,
        "provider operation never started"
    );
    assert_eq!(probe.failures.get(), 0);
    if matches!(kind, Kind::Request) {
        assert!(!witness.disposed.load(Ordering::SeqCst));
    }
    drop(connection);
    match kind {
        Kind::Request => {
            assert!(
                wait_until(|| witness.disposed.load(Ordering::SeqCst)).await,
                "Hyper has not dropped the pending HTTP dispatch"
            );
            assert_eq!(
                witness.completed.load(Ordering::SeqCst),
                0,
                "a response completed before client disconnect"
            );
            eprintln!(
                "WEB_K1 request caller dropped by HTTP transport; context_cancelled={}",
                probe.context.borrow().as_ref().unwrap().is_cancelled()
            );
        }
        Kind::Stream => {
            assert!(
                wait_until(|| probe.cancels.get() == 1).await,
                "HTTP body disposal has not cancelled the session"
            );
            eprintln!("WEB_K1 HTTP stream cancelled; pending receive still owned");
        }
    }
    assert_eq!(probe.failures.get(), 0, "failure gate must still be closed");
    assert_eq!(
        probe.execution_drops.get(),
        0,
        "pending provider future must remain owned after caller disposal"
    );
    finish.send(()).unwrap();
    assert!(wait_until(|| probe.failures.get() == 1).await);
    let restarted = wait_until(|| app.plugin_generation("provider") == Some(2)).await;
    eprintln!(
        "WEB_K1 late failure observed generation={:?} instantiations={} calls={} failures={}",
        app.plugin_generation("provider"),
        probe.instantiations.get(),
        probe.calls.get(),
        probe.failures.get()
    );
    let mut healthy_response = String::new();
    if restarted {
        let mut healthy = begin_request(address).await;
        tokio::time::timeout(BOUND, healthy.read_to_string(&mut healthy_response))
            .await
            .unwrap()
            .unwrap();
        eprintln!("WEB_K1 replacement HTTP response={healthy_response:?}");
    }
    let mut unavailable = Vec::new();
    let mut ready = Vec::new();
    while let Some(record) = observer.try_recv() {
        eprintln!("WEB_K1 supervision={:?}", record.event);
        match record.event {
            DiagnosticEvent::GenerationUnavailable {
                instance,
                generation,
            } if instance == "provider" => unavailable.push(generation),
            DiagnosticEvent::GenerationReady {
                instance,
                generation,
            } if instance == "provider" => ready.push(generation),
            _ => {}
        }
    }
    let final_generation = app.plugin_generation("provider");
    let terminal_failure = app.terminal_failure();
    let shutdown = tokio::time::timeout(BOUND, app.shutdown(Duration::from_secs(2)))
        .await
        .unwrap();
    let listener_closed = TcpStream::connect(address).await.is_err();
    eprintln!(
        "WEB_K1 cleanup shutdown={shutdown:?} listener_closed={listener_closed} session_drops={} pid={}",
        probe.session_drops.get(),
        std::process::id()
    );
    // Assert the bug after bounded cleanup so the red control also proves exit.
    assert_eq!(shutdown, ShutdownOutcome::Clean);
    assert!(listener_closed);
    assert!(
        restarted,
        "late PluginFailure disappeared after the real HTTP caller disconnected"
    );
    assert_eq!(
        probe.instantiations.get(),
        2,
        "exactly one replacement generation"
    );
    assert_eq!(
        probe.calls.get(),
        2,
        "one failed call and one explicit healthy call; no replay"
    );
    assert_eq!(probe.failures.get(), 1);
    assert_eq!(
        probe.execution_drops.get(),
        1,
        "provider execution must release after observed completion"
    );
    assert_eq!(final_generation, Some(2));
    assert_eq!(terminal_failure, None);
    assert_eq!(unavailable, [1]);
    assert_eq!(ready, [2]);
    assert_eq!(observer.dropped_count(), 0);
    assert!(healthy_response.starts_with("HTTP/1.1 200"));
    assert!(healthy_response.contains("generation-2"));
    if matches!(kind, Kind::Stream) {
        assert!(
            healthy_response.ends_with("0\r\n\r\n"),
            "healthy stream must terminate cleanly"
        );
        assert_eq!(probe.cancels.get(), 1);
        assert_eq!(probe.session_drops.get(), 2);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn disconnected_http_request_reports_late_v2_failure() {
    LocalSet::new()
        .run_until(tokio::time::timeout(
            Duration::from_secs(20),
            qualification(Kind::Request),
        ))
        .await
        .expect("bounded Request qualification");
}

#[tokio::test(flavor = "current_thread")]
async fn disconnected_http_stream_reports_late_v2_failure() {
    LocalSet::new()
        .run_until(tokio::time::timeout(
            Duration::from_secs(20),
            qualification(Kind::Stream),
        ))
        .await
        .expect("bounded Stream qualification");
}
