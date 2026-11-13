//! Native event delivery and Console descriptions derived from installed modules.

use std::sync::{Arc, Mutex};

use lenso_app::{
    Action, ActionError, Catalog, ConsoleManifest, ConsolePage, Context, ErrorCode, Event, Module,
};

struct Read;
impl Action<()> for Read {
    type Input = ();
    type Output = ();
    fn id(&self) -> &str {
        "files.read"
    }
    async fn authorize(&self, _: &Context<()>, _: &()) -> Result<(), ActionError> {
        Ok(())
    }
    async fn run(&self, _: &Context<()>, _: ()) -> Result<(), ActionError> {
        Ok(())
    }
}

#[test]
fn catalog_installation_is_atomic_and_console_references_are_checked() {
    let mut module = Module::new("files");
    module.action(&Read).resource("db");
    let mut catalog = Catalog::new();
    catalog.register(module.clone()).unwrap();
    assert_eq!(
        catalog.require_resources([]).unwrap_err().code(),
        ErrorCode::NotFound
    );
    catalog.require_resources(["db"]).unwrap();
    assert_eq!(
        catalog.register(module).unwrap_err().code(),
        ErrorCode::Conflict
    );
    assert_eq!(catalog.describe().len(), 1);
    let mut duplicate = Module::new("other");
    duplicate.action(&Read);
    assert_eq!(
        catalog.register(duplicate).unwrap_err().code(),
        ErrorCode::Conflict
    );
    let page = ConsolePage {
        id: "files".into(),
        title: "Files".into(),
        list: "files.read".into(),
        detail: None,
        actions: vec![],
    };
    let manifest = ConsoleManifest::new(&catalog, vec![page.clone()]).unwrap();
    assert_eq!(manifest.version, 1);
    assert_eq!(
        ConsoleManifest::new(
            &catalog,
            vec![ConsolePage {
                list: "missing".into(),
                ..page
            }]
        )
        .unwrap_err()
        .code(),
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn local_events_await_in_order_support_unsubscribe_and_stop_on_failure() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut event = Event::new("user.created");
    let first = seen.clone();
    let subscription = event.subscribe(move |value: String| {
        let seen = first.clone();
        async move {
            tokio::task::yield_now().await;
            seen.lock().unwrap().push(value);
            Ok(())
        }
    });
    let second = seen.clone();
    event.subscribe(move |value: String| {
        let seen = second.clone();
        async move {
            seen.lock().unwrap().push(value.to_uppercase());
            Ok(())
        }
    });
    event.publish("alice".to_owned()).await.unwrap();
    event.unsubscribe(subscription);
    event.publish("bob".to_owned()).await.unwrap();
    assert_eq!(*seen.lock().unwrap(), ["alice", "ALICE", "BOB"]);
    event.subscribe(|_| async { Err(ActionError::new(ErrorCode::Internal, "Subscriber failed")) });
    event.subscribe(|_| async { panic!("delivery must stop after the first failure") });
    assert_eq!(
        event.publish("carol".into()).await.unwrap_err().code(),
        ErrorCode::Internal
    );
}
