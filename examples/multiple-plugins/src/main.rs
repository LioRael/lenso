use lenso_multiple_plugins_example::health;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), lenso_web_host::WebHostError> {
    lenso_web_host::NativeWebHost::new()
        .plugin::<health::Plugin>()
        .bind("127.0.0.1:8080".parse().expect("static address"))
        .run()
        .await
}
