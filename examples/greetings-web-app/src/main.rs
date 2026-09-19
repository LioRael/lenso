use lenso_web_greetings_plugin_example::GreetingsHttp;
use lenso_web_host::NativeWebHost;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), lenso_web_host::WebHostError> {
    NativeWebHost::new()
        .plugin::<GreetingsHttp>()
        .bind("127.0.0.1:8080".parse().expect("static bind address"))
        .run()
        .await
}
