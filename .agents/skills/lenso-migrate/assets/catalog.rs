use lenso_plugin_core::{
    AccessError, App, BoxError, CleanupPhase, Handle, Instance, InstanceKey, Plugin, Setup,
    WorkGate,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Events = Arc<Mutex<Vec<String>>>;
fn verify_closed(events: &Events) {
    let events = events.lock().unwrap();
    let mut sorted = events.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["north", "query-north", "query-south", "south"]);
    for (query, source) in [("query-north", "north"), ("query-south", "south")] {
        assert!(
            events.iter().position(|e| e == query).unwrap()
                < events.iter().position(|e| e == source).unwrap()
        );
    }
}
struct CatalogPlugin {
    id: &'static str,
    value: &'static str,
    events: Events,
}
struct Catalog {
    records: Mutex<Option<HashMap<String, String>>>,
}
impl Catalog {
    // Dependency-local call. The outer Query owns admission.
    async fn lookup(&self, key: &str) -> Result<Option<String>, BoxError> {
        let records = self.records.lock().map_err(|_| "catalog lock failed")?;
        Ok(records.as_ref().ok_or("catalog closed")?.get(key).cloned())
    }
}
impl Plugin for CatalogPlugin {
    type Service = Catalog;
    async fn setup(self, setup: &mut Setup) -> Result<Arc<Catalog>, BoxError> {
        let service = Arc::new(Catalog {
            records: Mutex::new(Some(HashMap::from([("shared".into(), self.value.into())]))),
        });
        let owned = service.clone();
        setup.on_cleanup(CleanupPhase::Resources, move || async move {
            owned
                .records
                .lock()
                .map_err(|_| "catalog lock failed")?
                .take();
            self.events
                .lock()
                .map_err(|_| "events lock failed")?
                .push(self.id.into());
            Ok(())
        });
        Ok(service)
    }
}
struct QueryPlugin {
    id: &'static str,
    source: Handle<Catalog>,
    fail: bool,
    events: Events,
}
struct Query {
    source: Arc<Catalog>,
    gate: WorkGate,
}
impl Query {
    async fn lookup(&self, key: &str) -> Result<Option<String>, BoxError> {
        let _work = self.gate.enter()?; // retain through dependency await
        self.source.lookup(key).await
    }
}
impl Plugin for QueryPlugin {
    type Service = Query;
    fn dependencies(&self) -> Vec<InstanceKey> {
        vec![self.source.key()]
    }
    async fn setup(self, setup: &mut Setup) -> Result<Arc<Query>, BoxError> {
        let source = setup.get(&self.source)?;
        setup.on_cleanup(CleanupPhase::Resources, move || async move {
            self.events
                .lock()
                .map_err(|_| "events lock failed")?
                .push(self.id.into());
            Ok(())
        });
        if self.fail {
            return Err("deliberate setup failure".into());
        }
        Ok(Arc::new(Query {
            source,
            gate: setup.work_gate(),
        }))
    }
}
// Application-owned aggregate, not a framework API; retains both original causes.
#[derive(Debug)]
struct RunErrors(Vec<BoxError>);
impl std::fmt::Display for RunErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native example failed ({} causes)", self.0.len())
    }
}
impl std::error::Error for RunErrors {}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), BoxError> {
    let fail = std::env::args().any(|a| a == "--fail");
    let events: Events = Arc::new(Mutex::new(vec![]));
    let north = Instance::new(
        "north",
        CatalogPlugin {
            id: "north",
            value: "North Ada",
            events: events.clone(),
        },
    );
    let south = Instance::new(
        "south",
        CatalogPlugin {
            id: "south",
            value: "South Grace",
            events: events.clone(),
        },
    );
    let a = Instance::new(
        "query-north",
        QueryPlugin {
            id: "query-north",
            source: north.handle(),
            fail: false,
            events: events.clone(),
        },
    );
    let b = Instance::new(
        "query-south",
        QueryPlugin {
            id: "query-south",
            source: south.handle(),
            fail,
            events: events.clone(),
        },
    );
    let (a_handle, b_handle) = (a.handle(), b.handle());
    let running = match App::new()
        .with(a)
        .with(b)
        .with(north)
        .with(south)
        .start()
        .await
    {
        Ok(app) => app,
        Err(error) if fail => {
            assert!(error.to_string().contains("deliberate setup failure"));
            verify_closed(&events);
            println!(
                "{{\"rollback\":{}}}",
                serde_json::to_string(&*events.lock().unwrap())?
            );
            return Ok(());
        }
        Err(error) => return Err(Box::new(error) as BoxError),
    };
    let outcome = async {
        if fail {
            return Err("expected setup failure".into());
        }
        let service = running.get(&a_handle)?;
        let values = vec![
            service.lookup("shared").await?,
            running.get(&b_handle)?.lookup("shared").await?,
        ];
        assert_eq!(
            values,
            vec![Some("North Ada".into()), Some("South Grace".into())]
        );
        assert_eq!(service.lookup("missing").await?, None);
        Ok::<_, BoxError>(service)
    }
    .await;
    let shutdown = running.shutdown().await;
    let service = match (outcome, shutdown) {
        (Ok(service), Ok(())) => service,
        (outcome, shutdown) => {
            let mut errors = vec![];
            if let Err(e) = outcome {
                errors.push(e);
            }
            if let Err(e) = shutdown {
                errors.push(Box::new(e) as BoxError);
            }
            return Err(Box::new(RunErrors(errors)) as BoxError);
        }
    };
    let error = service.lookup("shared").await.unwrap_err();
    assert!(matches!(
        error.downcast_ref::<AccessError>(),
        Some(AccessError::Closing)
    ));
    verify_closed(&events);
    println!(
        "{}",
        serde_json::json!({"values": ["North Ada", "South Grace"], "closed": *events.lock().unwrap()})
    );
    Ok(())
}
