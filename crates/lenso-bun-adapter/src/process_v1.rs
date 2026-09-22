//! Host-owned mechanics for the Generic Process Protocol V1.

use std::time::Duration;

use lenso_process_protocol::{ReadinessRecord, decode_strict};

#[cfg(unix)]
use std::{
    io::{Read as _, Write as _},
    os::unix::process::CommandExt as _,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
};

#[cfg(unix)]
use command_fds::{CommandFdExt as _, FdMapping};

#[cfg(unix)]
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};

#[cfg(unix)]
mod client;
#[cfg(unix)]
pub use client::{
    ClientError, ProcessV1Session, ProcessV1ValidationError, ProcessV1ValueValidator,
    ShutdownDiagnostic, authenticate_process_v1,
};
mod terminal;
pub use terminal::{
    InvocationTerminal, ShutdownPhase, TerminalArbiter, TerminalError, TerminalKind,
};

/// Parses exactly one bounded readiness line and validates distinct listeners.
pub fn parse_readiness_line(
    wire: &[u8],
    maximum_bytes: usize,
) -> Result<ReadinessRecord, ReadinessError> {
    if wire.is_empty() || wire.len() > maximum_bytes {
        return Err(ReadinessError::Size);
    }
    let line = std::str::from_utf8(wire).map_err(|_| ReadinessError::Encoding)?;
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() || line.contains(['\r', '\n']) {
        return Err(ReadinessError::RecordCount);
    }
    let readiness =
        decode_strict::<ReadinessRecord>(line.as_bytes()).map_err(|_| ReadinessError::Document)?;
    readiness.validate().map_err(|error| {
        if error.detail().contains("profile") {
            ReadinessError::Profile
        } else {
            ReadinessError::Listeners
        }
    })?;
    Ok(readiness)
}

/// Stable host diagnostic for readiness rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessError {
    Size,
    Encoding,
    RecordCount,
    Document,
    Profile,
    Listeners,
}

/// One-use host secret retained only until the authenticated handshake finishes.
pub struct BootstrapSecret([u8; 32]);

impl BootstrapSecret {
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for BootstrapSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BootstrapSecret(<redacted>)")
    }
}

impl Drop for BootstrapSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[cfg(unix)]
#[derive(Debug)]
pub struct SpawnedProcessV1 {
    child: Option<Child>,
    pub readiness: ReadinessRecord,
    bootstrap_secret: Option<BootstrapSecret>,
}

#[cfg(unix)]
impl SpawnedProcessV1 {
    pub fn child(&self) -> &Child {
        self.child.as_ref().expect("spawned process owns its child")
    }

    pub fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("spawned process owns its child")
    }

    pub fn bootstrap_secret(&self) -> &BootstrapSecret {
        self.bootstrap_secret
            .as_ref()
            .expect("spawned process owns its bootstrap secret")
    }

    pub(super) fn retire(&mut self) {
        if let Some(mut child) = self.child.take() {
            retire_process_group(&mut child);
        }
        self.bootstrap_secret.take();
    }

    pub(super) fn into_parts(
        mut self,
    ) -> Result<(Child, ReadinessRecord, BootstrapSecret), SpawnError> {
        let child = self.child.take().ok_or(SpawnError::Launch)?;
        let secret = self.bootstrap_secret.take().ok_or(SpawnError::Random)?;
        Ok((child, self.readiness.clone(), secret))
    }
}

#[cfg(unix)]
impl Drop for SpawnedProcessV1 {
    fn drop(&mut self) {
        self.retire();
    }
}

/// Spawns one process group and exchanges bootstrap/readiness only on inherited pipes.
#[cfg(unix)]
pub fn spawn_process_v1(
    mut command: Command,
    limits: &HostProcessLimits,
) -> Result<SpawnedProcessV1, SpawnError> {
    limits.validate().map_err(|_| SpawnError::Limits)?;
    if command.get_current_dir().is_none() {
        return Err(SpawnError::WorkingDirectory);
    }
    let inherited = ["PATH", "TMPDIR", "LANG", "LC_ALL", "TZ"]
        .into_iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (name, value)))
        .collect::<Vec<_>>();
    command.env_clear().envs(inherited);
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret).map_err(|_| SpawnError::Random)?;
    let (secret_reader, mut secret_writer) = std::io::pipe().map_err(|_| SpawnError::Pipe)?;
    let (mut readiness_reader, readiness_writer) = std::io::pipe().map_err(|_| SpawnError::Pipe)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    command
        .fd_mappings(vec![
            FdMapping {
                parent_fd: secret_reader.into(),
                child_fd: 3,
            },
            FdMapping {
                parent_fd: readiness_writer.into(),
                child_fd: 4,
            },
        ])
        .map_err(|_| SpawnError::Pipe)?;
    let mut child = command.spawn().map_err(|_| SpawnError::Launch)?;
    drop(command);
    if secret_writer.write_all(&secret).is_err() {
        retire_process_group(&mut child);
        secret.fill(0);
        return Err(SpawnError::BootstrapWrite);
    }
    drop(secret_writer);

    let maximum = limits.readiness_record_bytes;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("lenso-process-v1-readiness".to_owned())
        .spawn(move || {
            let mut wire = Vec::with_capacity(maximum.min(4_096));
            let result = readiness_reader
                .by_ref()
                .take(u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1))
                .read_to_end(&mut wire)
                .map(|_| wire);
            let _ = sender.send(result);
        })
        .map_err(|_| {
            retire_process_group(&mut child);
            secret.fill(0);
            SpawnError::ReadinessThread
        })?;
    let wire = match receiver.recv_timeout(limits.startup_timeout) {
        Ok(Ok(wire)) => wire,
        Ok(Err(_)) => {
            retire_process_group(&mut child);
            secret.fill(0);
            return Err(SpawnError::ReadinessRead);
        }
        Err(_) => {
            retire_process_group(&mut child);
            secret.fill(0);
            return Err(SpawnError::StartupTimeout);
        }
    };
    let Ok(readiness) = parse_readiness_line(&wire, maximum) else {
        retire_process_group(&mut child);
        secret.fill(0);
        return Err(SpawnError::ReadinessDocument);
    };
    Ok(SpawnedProcessV1 {
        child: Some(child),
        readiness,
        bootstrap_secret: Some(BootstrapSecret(secret)),
    })
}

#[cfg(unix)]
pub(super) fn retire_process_group(child: &mut Child) {
    let group = i32::try_from(child.id()).ok().map(Pid::from_raw);
    if let Some(group) = group {
        let _ = killpg(group, Signal::SIGKILL);
    } else {
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[cfg(unix)]
pub(super) fn process_group_exists(child_id: u32) -> bool {
    i32::try_from(child_id).is_ok_and(|group| kill(Pid::from_raw(-group), None).is_ok())
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnError {
    Limits,
    Random,
    Pipe,
    Launch,
    WorkingDirectory,
    BootstrapWrite,
    ReadinessThread,
    ReadinessRead,
    ReadinessDocument,
    StartupTimeout,
}

/// Host-only bounds used around bootstrap, cancellation, and shutdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostProcessLimits {
    pub readiness_record_bytes: usize,
    pub startup_timeout: Duration,
    pub cancel_ack_timeout: Duration,
    pub shutdown_ack_timeout: Duration,
    pub shutdown_exit_timeout: Duration,
    pub term_grace: Duration,
    pub kill_reap_timeout: Duration,
}

impl Default for HostProcessLimits {
    fn default() -> Self {
        Self {
            readiness_record_bytes: 4_096,
            startup_timeout: Duration::from_secs(30),
            cancel_ack_timeout: Duration::from_secs(1),
            shutdown_ack_timeout: Duration::from_secs(5),
            shutdown_exit_timeout: Duration::from_secs(5),
            term_grace: Duration::from_secs(5),
            kill_reap_timeout: Duration::from_secs(5),
        }
    }
}

impl HostProcessLimits {
    pub fn validate(&self) -> Result<(), HostLimitError> {
        validate_host_limit(self.readiness_record_bytes, 65_536)?;
        validate_duration(self.startup_timeout, Duration::from_secs(300))?;
        validate_duration(self.cancel_ack_timeout, Duration::from_secs(10))?;
        validate_duration(self.shutdown_ack_timeout, Duration::from_secs(30))?;
        validate_duration(self.shutdown_exit_timeout, Duration::from_secs(30))?;
        validate_duration(self.term_grace, Duration::from_secs(30))?;
        validate_duration(self.kill_reap_timeout, Duration::from_secs(30))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostLimitError {
    Zero,
    AboveProfileMaximum,
}

fn validate_host_limit(value: usize, maximum: usize) -> Result<(), HostLimitError> {
    if value == 0 {
        Err(HostLimitError::Zero)
    } else if value > maximum {
        Err(HostLimitError::AboveProfileMaximum)
    } else {
        Ok(())
    }
}

fn validate_duration(value: Duration, maximum: Duration) -> Result<(), HostLimitError> {
    if value.is_zero() {
        Err(HostLimitError::Zero)
    } else if value > maximum {
        Err(HostLimitError::AboveProfileMaximum)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn readiness_requires_the_exact_profile_and_distinct_ports() {
        let record = br#"{"protocol":"lenso-process-jsonrpc-http-v1","data_port":31001,"control_port":31002}"#;
        let parsed = parse_readiness_line(record, 4096).unwrap();
        assert_eq!(parsed.data_port, 31001);
        assert_eq!(parsed.control_port, 31002);

        let same = br#"{"protocol":"lenso-process-jsonrpc-http-v1","data_port":31001,"control_port":31001}"#;
        assert_eq!(
            parse_readiness_line(same, 4096),
            Err(ReadinessError::Listeners)
        );
        assert_eq!(
            parse_readiness_line(b"{}\n{}", 4096),
            Err(ReadinessError::RecordCount)
        );
    }

    #[test]
    fn cancellation_wins_over_deadline_and_response() {
        let now = Instant::now();
        let mut arbiter = TerminalArbiter::new(8);
        arbiter
            .admit(1, Some(now.checked_sub(Duration::from_nanos(1)).unwrap()))
            .unwrap();
        assert_eq!(
            arbiter.cancel::<&str>(1).unwrap(),
            Some(InvocationTerminal::Cancelled)
        );
        assert_eq!(
            arbiter.respond(1, "late", now),
            Err(TerminalError::SecondTerminal)
        );
    }

    #[test]
    fn deadline_wins_over_a_response_in_the_commit_section() {
        let now = Instant::now();
        let mut arbiter = TerminalArbiter::new(8);
        arbiter.admit(2, Some(now)).unwrap();
        assert_eq!(
            arbiter.respond(2, "late", now).unwrap(),
            InvocationTerminal::DeadlineExceeded
        );
    }

    #[test]
    fn retired_ids_are_never_evicted_or_reused() {
        let mut arbiter = TerminalArbiter::new(1);
        arbiter.admit(1, None).unwrap();
        assert_eq!(
            arbiter.respond(1, "done", Instant::now()).unwrap(),
            InvocationTerminal::Response("done")
        );
        assert_eq!(
            arbiter.admit(1, None),
            Err(TerminalError::ReusedCorrelationId)
        );
        assert_eq!(
            arbiter.admit(2, None),
            Err(TerminalError::RetiredIdCapacity)
        );
    }

    #[test]
    fn value_profile_constant_remains_the_portable_contract_profile() {
        assert_eq!(crate::protocol::VALUE_PROFILE, "lenso-json-value-v1");
    }

    #[test]
    fn host_limits_reject_zero_and_profile_overflow() {
        HostProcessLimits::default().validate().unwrap();
        let mut invalid = HostProcessLimits {
            readiness_record_bytes: 0,
            ..HostProcessLimits::default()
        };
        assert_eq!(invalid.validate(), Err(HostLimitError::Zero));
        invalid.readiness_record_bytes = 65_537;
        assert_eq!(invalid.validate(), Err(HostLimitError::AboveProfileMaximum));
    }

    #[test]
    fn shutdown_escalation_is_monotonic_and_terminal() {
        let mut phase = ShutdownPhase::CloseAdmission;
        for _ in 0..7 {
            let next = phase.next();
            assert!(next >= phase);
            phase = next;
        }
        assert_eq!(phase, ShutdownPhase::Complete);
        assert_eq!(phase.next(), ShutdownPhase::Complete);
    }
}
