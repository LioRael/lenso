mod business_source;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context as _, bail};
use lenso_web_greetings_plugin_example::GreetingsHttp;
use lenso_web_host::NativeWebHost;
use tokio::task::LocalSet;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut policy: Option<PathBuf> = None;
    let mut bind: SocketAddr = "127.0.0.1:8080".parse().expect("static bind address");
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--business-policy") if policy.is_none() => {
                policy = Some(PathBuf::from(
                    args.next().context("--business-policy needs a path")?,
                ));
            }
            Some("--bind") => {
                bind = args
                    .next()
                    .context("--bind needs an address")?
                    .to_str()
                    .context("--bind must be UTF-8")?
                    .parse()
                    .context("--bind needs a socket address")?;
            }
            _ => bail!("usage: greetings-web [--business-policy ABSOLUTE_PATH] [--bind ADDRESS]"),
        }
    }
    match policy {
        Some(policy) => {
            LocalSet::new()
                .run_until(business_source::run(policy, bind, None))
                .await
        }
        None => NativeWebHost::new()
            .plugin::<GreetingsHttp>()
            .bind(bind)
            .run()
            .await
            .map_err(Into::into),
    }
}
