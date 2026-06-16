use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
pub struct Shutdown {
    sender: Arc<watch::Sender<bool>>,
    receiver: watch::Receiver<bool>,
}

impl Debug for Shutdown {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Shutdown").finish_non_exhaustive()
    }
}

impl Shutdown {
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self {
            sender: Arc::new(sender),
            receiver,
        }
    }

    pub fn signal(&self) {
        let _ = self.sender.send(true);
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.receiver.clone()
    }

    pub async fn wait_for_signal() {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            () = ctrl_c => {},
            () = terminate => {},
        }
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}
