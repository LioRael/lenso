use std::{fs::OpenOptions, io::Write};

fn record(event: &str) {
    let path = std::env::var_os("LENSO_TEST_BINDING_EVENTS").expect("event path");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{event}").unwrap();
}

#[lenso::plugin(consumer, lifecycle)]
#[derive(Clone, Debug)]
struct Observer {}

impl lenso::Lifecycle for Observer {
    async fn activate(&self, _: lenso::ActivateContext) -> Result<(), lenso::RuntimeFailure> {
        record("activate");
        Ok(())
    }

    async fn deactivate(&self, _: lenso::DeactivateContext) -> Result<(), lenso::RuntimeFailure> {
        record("deactivate");
        if std::env::var("LENSO_TEST_BINDING_STOP").as_deref() == Ok("stop-failure") {
            return Err(lenso::RuntimeFailure::PluginFailure {
                detail: "fixture stop rejected".to_owned(),
            });
        }
        Ok(())
    }
}

pub mod business_snapshot {
    use std::path::Path;

    use lenso_app_plan::ResolvedAppPlan;
    use lenso_native_adapter::NativePluginRegistry;

    pub struct Poller {
        fail_recheck: bool,
    }

    pub struct Guard;

    pub fn bind(
        registry: NativePluginRegistry,
        _plan: &ResolvedAppPlan,
        path: &Path,
    ) -> anyhow::Result<(NativePluginRegistry, Poller)> {
        super::record("bind");
        let mode = std::fs::read_to_string(path)?;
        anyhow::ensure!(mode != "bind-failure", "fixture bind rejected");
        Ok((
            registry,
            Poller {
                fail_recheck: mode == "recheck-failure",
            },
        ))
    }

    impl Poller {
        pub async fn recheck(&self) -> anyhow::Result<()> {
            super::record("recheck");
            anyhow::ensure!(!self.fail_recheck, "fixture recheck rejected");
            Ok(())
        }

        pub fn spawn(self) -> Guard {
            super::record("spawn");
            Guard
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(receipt) = std::env::var_os("LENSO_MANAGED_SHUTDOWN_RECEIPT") {
                assert!(
                    !Path::new(&receipt).exists(),
                    "receipt preceded binding guard cleanup"
                );
            }
            super::record("drop");
            assert_ne!(
                std::env::var("LENSO_TEST_BINDING_STOP").as_deref(),
                Ok("panic"),
                "fixture guard cleanup panicked"
            );
        }
    }
}
