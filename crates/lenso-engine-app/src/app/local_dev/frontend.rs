//! Explicit, App-owned frontend development process. This is tooling, not an
//! App Composition input or a declaration that checked-in static assets changed.
use std::{
    fs,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail, ensure};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::Child,
};
use url::Url;

const CONFIG: &str = "frontend/lenso.dev.toml";

pub(super) struct FrontendProcess {
    child: Child,
    group_id: u32,
}

impl FrontendProcess {
    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }
}

pub(super) async fn stop(process: &mut FrontendProcess) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, killpg},
            unistd::Pid,
        };
        let group = Pid::from_raw(i32::try_from(process.group_id)?);
        if let Err(error) = killpg(group, Signal::SIGTERM)
            && error != nix::errno::Errno::ESRCH
        {
            return Err(error.into());
        }
        let waited = tokio::time::timeout(Duration::from_secs(12), process.child.wait()).await;
        if let Err(error) = killpg(group, Signal::SIGKILL)
            && error != nix::errno::Errno::ESRCH
        {
            return Err(error.into());
        }
        match waited {
            Ok(result) => {
                result?;
            }
            Err(_) => {
                process.child.wait().await?;
                bail!("frontend dev process group did not stop within its shutdown budget");
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        super::stop(&mut process.child, true).await
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FrontendConfig {
    schema: String,
    command: Vec<String>,
    url: String,
    backend_url_mode: String,
}

impl FrontendConfig {
    pub(super) fn load(root: &Path) -> anyhow::Result<Option<Self>> {
        let path = root.join(CONFIG);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("inspect frontend dev configuration"),
        };
        ensure!(
            metadata.file_type().is_file() && metadata.len() <= 16 * 1024,
            "frontend dev configuration must be a regular file no larger than 16 KiB"
        );
        let directory = root.join("frontend");
        ensure!(
            fs::symlink_metadata(&directory)?.file_type().is_dir(),
            "frontend development directory must be an App-owned directory"
        );
        let config: Self = toml::from_str(&fs::read_to_string(path)?)?;
        ensure!(
            config.schema == "lenso.frontend-dev.v1",
            "unsupported frontend dev schema"
        );
        ensure!(
            !config.command.is_empty()
                && config.command.len() <= 32
                && config
                    .command
                    .iter()
                    .all(|part| !part.is_empty() && part.len() <= 4096),
            "frontend dev command must contain 1 to 32 bounded arguments"
        );
        ensure!(
            config.backend_url_mode == "file",
            "frontend dev must declare backend_url_mode = 'file'"
        );
        config.address()?;
        Ok(Some(config))
    }

    fn address(&self) -> anyhow::Result<SocketAddrV4> {
        let url = Url::parse(&self.url).context("parse frontend dev preview URL")?;
        ensure!(
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some_and(|port| port != 0)
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none(),
            "frontend dev preview URL must be http://127.0.0.1:<port>/"
        );
        Ok(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            url.port().context("frontend dev port")?,
        ))
    }

    pub(super) fn preview_url(&self) -> &str {
        &self.url
    }

    pub(super) async fn launch(
        &self,
        root: &Path,
        backend_url: &str,
        backend_file: &Path,
    ) -> anyhow::Result<Option<FrontendProcess>> {
        let address = self.address()?;
        // Refuse a pre-existing listener before starting the configured process.
        // The subsequent HTTP probe also requires the child to remain alive.
        TcpListener::bind(address)
            .with_context(|| format!("frontend preview port {address} is already in use"))?;
        let mut command = super::command(PathBuf::from(&self.command[0]));
        command
            .args(&self.command[1..])
            .current_dir(root.join("frontend"));
        command.env_clear();
        for name in [
            "PATH", "HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "TERM",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("LENSO_API_URL", backend_url);
        command.env("LENSO_API_URL_FILE", backend_file);
        let child = command
            .spawn()
            .context("start explicit frontend dev command")?;
        let mut process = FrontendProcess {
            group_id: child.id().context("frontend dev process ID")?,
            child,
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = process.try_wait()? {
                stop(&mut process).await?;
                bail!("frontend dev command exited before preview readiness: {status}");
            }
            if probe(address, "/", None).await.unwrap_or(false)
                && probe(address, "/__lenso/backend", Some(backend_url))
                    .await
                    .unwrap_or(false)
            {
                if process.try_wait()?.is_some() {
                    stop(&mut process).await?;
                    bail!("frontend dev command exited at preview readiness");
                }
                return Ok(Some(process));
            }
            if tokio::time::Instant::now() >= deadline {
                stop(&mut process).await?;
                bail!("frontend preview did not become ready within 30 seconds");
            }
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    stop(&mut process).await?;
                    return Ok(None);
                }
                () = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
    }

    pub(super) async fn verify_backend(
        &self,
        process: &mut FrontendProcess,
        backend_url: &str,
    ) -> anyhow::Result<bool> {
        let address = self.address()?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = process.try_wait()? {
                bail!("frontend dev process exited before backend refresh: {status}");
            }
            if probe(address, "/__lenso/backend", Some(backend_url))
                .await
                .unwrap_or(false)
            {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("frontend dev server did not confirm the new backend URL within 5 seconds");
            }
            tokio::select! {
                signal = tokio::signal::ctrl_c() => { signal?; return Ok(false); }
                () = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
    }
}

async fn probe(
    address: SocketAddrV4,
    path: &str,
    expected_body: Option<&str>,
) -> anyhow::Result<bool> {
    let operation = async {
        let mut stream = TcpStream::connect(address).await?;
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await?;
        let mut response = Vec::new();
        stream.take(2048).read_to_end(&mut response).await?;
        let status =
            response.starts_with(b"HTTP/1.1 200 ") || response.starts_with(b"HTTP/1.0 200 ");
        let body = response
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .map(|offset| &response[offset + 4..]);
        Ok::<_, std::io::Error>(
            status
                && expected_body.is_none_or(|expected| {
                    body.is_some_and(|body| {
                        body == expected.as_bytes() || body == format!("{expected}\n").as_bytes()
                    })
                }),
        )
    };
    Ok(tokio::time::timeout(Duration::from_millis(500), operation).await??)
}

pub(super) fn is_config(root: &Path, path: &Path) -> bool {
    path == root.join(CONFIG)
}

pub(super) fn is_frontend(root: &Path, path: &Path) -> bool {
    path.starts_with(root.join("frontend"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_dev_is_opt_in_and_loopback_only() {
        let root = tempfile::tempdir().unwrap();
        assert!(FrontendConfig::load(root.path()).unwrap().is_none());
        fs::create_dir(root.path().join("frontend")).unwrap();
        let config = root.path().join(CONFIG);
        fs::write(
            &config,
            "schema = 'lenso.frontend-dev.v1'\ncommand = ['bun', 'run', 'dev']\nurl = 'http://127.0.0.1:5173/'\nbackend_url_mode = 'file'\n",
        )
        .unwrap();
        assert_eq!(
            FrontendConfig::load(root.path())
                .unwrap()
                .unwrap()
                .preview_url(),
            "http://127.0.0.1:5173/"
        );
        fs::write(
            &config,
            "schema = 'lenso.frontend-dev.v1'\ncommand = ['bun', 'run', 'dev']\nurl = 'http://0.0.0.0:5173/'\nbackend_url_mode = 'file'\n",
        )
        .unwrap();
        assert!(FrontendConfig::load(root.path()).is_err());
    }

    #[tokio::test]
    async fn readiness_requires_an_http_200_from_the_declared_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(address) => address,
            _ => unreachable!(),
        };
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 128];
            let count = stream.read(&mut request).await.unwrap();
            assert!(request[..count].starts_with(b"GET / HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        assert!(probe(address, "/", None).await.unwrap());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn backend_handshake_rejects_a_stale_target() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(address) => address,
            _ => unreachable!(),
        };
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 128];
            let count = stream.read(&mut request).await.unwrap();
            assert!(request[..count].starts_with(b"GET /__lenso/backend HTTP/1.1"));
            let body = b"http://127.0.0.1:3001/\n";
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            stream.write_all(body).await.unwrap();
        });
        assert!(
            !probe(address, "/__lenso/backend", Some("http://127.0.0.1:3002/"))
                .await
                .unwrap()
        );
        server.await.unwrap();
    }

    #[cfg(unix)]
    async fn assert_process_stopped(pid: i32) {
        use nix::{
            errno::Errno,
            sys::signal::{Signal, kill},
            unistd::Pid,
        };
        let pid = Pid::from_raw(pid);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if kill(pid, None) == Err(Errno::ESRCH) {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = kill(pid, Some(Signal::SIGKILL));
                panic!("frontend child process was not stopped");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exited_wrapper_does_not_leave_its_frontend_child_running() {
        let root = tempfile::tempdir().unwrap();
        let frontend = root.path().join("frontend");
        fs::create_dir(&frontend).unwrap();
        fs::write(
            frontend.join("wrapper.sh"),
            "sleep 30 &\nprintf '%s\\n' \"$!\" > child.pid\nexit 23\n",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let config = FrontendConfig {
            schema: "lenso.frontend-dev.v1".into(),
            command: vec!["/bin/sh".into(), "./wrapper.sh".into()],
            url: format!("http://127.0.0.1:{port}/"),
            backend_url_mode: "file".into(),
        };
        let error = config
            .launch(
                root.path(),
                "http://127.0.0.1:3001/",
                &root.path().join(".lenso/dev-backend-url"),
            )
            .await
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("exited before preview readiness"));
        let pid: i32 = fs::read_to_string(frontend.join("child.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_process_stopped(pid).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unexpected_dev_error_stops_the_frontend_process_group() {
        let root = tempfile::tempdir().unwrap();
        let frontend = root.path().join("frontend");
        fs::create_dir(&frontend).unwrap();
        fs::write(
            frontend.join("wrapper.sh"),
            "sleep 30 &\nprintf '%s\\n' \"$!\" > child.pid\nwait\n",
        )
        .unwrap();
        let mut command = super::super::command(PathBuf::from("/bin/sh"));
        command.arg("./wrapper.sh").current_dir(&frontend);
        let child = command.spawn().unwrap();
        let group_id = child.id().unwrap();
        let mut process = Some(FrontendProcess { child, group_id });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !frontend.join("child.pid").is_file() {
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let pid: i32 = fs::read_to_string(frontend.join("child.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut host = None;
        let error = super::super::finish_dev(
            Err(anyhow::anyhow!("simulated rebuild failure")),
            &mut host,
            &mut process,
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("simulated rebuild failure"));
        assert!(process.is_none());
        assert_process_stopped(pid).await;
    }
}
