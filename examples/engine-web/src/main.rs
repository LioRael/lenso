#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), lenso_web_host::WebHostError> {
    lenso_web_host::NativeWebHost::new()
        .plugin::<lenso_engine_web_example::Plugin>()
        .bind("127.0.0.1:18087".parse().expect("loopback"))
        .run()
        .await
}
