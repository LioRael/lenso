use std::{error::Error, net::SocketAddr};

use lenso_onboarding_todo_http::TodoHttp;
use lenso_web_host::NativeWebHost;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind: SocketAddr = match args.as_slice() {
        [] => "127.0.0.1:8080".parse()?,
        [flag, address] if flag == "--bind" => address.parse()?,
        _ => return Err("usage: todo-http [--bind 127.0.0.1:8080]".into()),
    };
    tokio::task::LocalSet::new()
        .run_until(async move {
            let running = NativeWebHost::new()
                .plugin::<TodoHttp>()
                .bind(bind)
                .start()
                .await?;
            println!("LISTENING http://{}", running.address());
            tokio::signal::ctrl_c().await?;
            running.shutdown().await?;
            Ok(())
        })
        .await
}
