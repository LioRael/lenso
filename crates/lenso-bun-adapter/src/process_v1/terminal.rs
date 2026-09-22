use std::{collections::BTreeMap, time::Instant};

/// Monotonic host-owned shutdown escalation; child output cannot move it backward.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ShutdownPhase {
    CloseAdmission,
    CancelInflight,
    AwaitShutdownAck,
    AwaitProcessExit,
    GracefulGroupTermination,
    ForcedGroupKill,
    BoundedReap,
    Complete,
}

impl ShutdownPhase {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::CloseAdmission => Self::CancelInflight,
            Self::CancelInflight => Self::AwaitShutdownAck,
            Self::AwaitShutdownAck => Self::AwaitProcessExit,
            Self::AwaitProcessExit => Self::GracefulGroupTermination,
            Self::GracefulGroupTermination => Self::ForcedGroupKill,
            Self::ForcedGroupKill => Self::BoundedReap,
            Self::BoundedReap | Self::Complete => Self::Complete,
        }
    }
}

/// Only the host can choose cancellation and deadline terminals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationTerminal<T> {
    Cancelled,
    DeadlineExceeded,
    Response(T),
}

#[derive(Debug)]
struct ActiveInvocation {
    deadline: Option<Instant>,
    cancelled: bool,
}

/// Serialized, bounded terminal commit authority for one child process session.
#[derive(Debug)]
pub struct TerminalArbiter {
    active: BTreeMap<u64, ActiveInvocation>,
    retired: BTreeMap<u64, TerminalKind>,
    maximum_retired: usize,
}

impl TerminalArbiter {
    pub fn new(maximum_retired: usize) -> Self {
        Self {
            active: BTreeMap::new(),
            retired: BTreeMap::new(),
            maximum_retired: maximum_retired.max(1),
        }
    }

    pub fn admit(
        &mut self,
        correlation_id: u64,
        deadline: Option<Instant>,
    ) -> Result<(), TerminalError> {
        if self.active.contains_key(&correlation_id) || self.retired.contains_key(&correlation_id) {
            return Err(TerminalError::ReusedCorrelationId);
        }
        if self.retired.len() >= self.maximum_retired {
            return Err(TerminalError::RetiredIdCapacity);
        }
        self.active.insert(
            correlation_id,
            ActiveInvocation {
                deadline,
                cancelled: false,
            },
        );
        Ok(())
    }

    pub fn cancel<T>(
        &mut self,
        correlation_id: u64,
    ) -> Result<Option<InvocationTerminal<T>>, TerminalError> {
        if self.retired.contains_key(&correlation_id) {
            return Ok(None);
        }
        let Some(active) = self.active.get_mut(&correlation_id) else {
            return Ok(None);
        };
        active.cancelled = true;
        self.commit(correlation_id, InvocationTerminal::Cancelled)
            .map(Some)
    }

    pub fn respond<T>(
        &mut self,
        correlation_id: u64,
        response: T,
        now: Instant,
    ) -> Result<InvocationTerminal<T>, TerminalError> {
        let active = self
            .active
            .get(&correlation_id)
            .ok_or_else(|| self.classify_missing(correlation_id))?;
        let terminal = if active.cancelled {
            InvocationTerminal::Cancelled
        } else if active.deadline.is_some_and(|deadline| now >= deadline) {
            InvocationTerminal::DeadlineExceeded
        } else {
            InvocationTerminal::Response(response)
        };
        self.commit(correlation_id, terminal)
    }

    pub fn expire<T>(
        &mut self,
        correlation_id: u64,
        now: Instant,
    ) -> Result<Option<InvocationTerminal<T>>, TerminalError> {
        let Some(active) = self.active.get(&correlation_id) else {
            return if self.retired.contains_key(&correlation_id) {
                Ok(None)
            } else {
                Err(TerminalError::UnknownCorrelationId)
            };
        };
        if active.cancelled {
            return self
                .commit(correlation_id, InvocationTerminal::Cancelled)
                .map(Some);
        }
        if active.deadline.is_some_and(|deadline| now >= deadline) {
            return self
                .commit(correlation_id, InvocationTerminal::DeadlineExceeded)
                .map(Some);
        }
        Ok(None)
    }

    fn commit<T>(
        &mut self,
        correlation_id: u64,
        terminal: InvocationTerminal<T>,
    ) -> Result<InvocationTerminal<T>, TerminalError> {
        if self.retired.len() >= self.maximum_retired {
            return Err(TerminalError::RetiredIdCapacity);
        }
        if self.active.remove(&correlation_id).is_none() {
            return Err(self.classify_missing(correlation_id));
        }
        self.retired
            .insert(correlation_id, TerminalKind::of(&terminal));
        Ok(terminal)
    }

    fn classify_missing(&self, correlation_id: u64) -> TerminalError {
        if self.retired.contains_key(&correlation_id) {
            TerminalError::SecondTerminal
        } else {
            TerminalError::UnknownCorrelationId
        }
    }

    pub fn retired_terminal(&self, correlation_id: u64) -> Option<TerminalKind> {
        self.retired.get(&correlation_id).copied()
    }

    pub fn active_ids(&self) -> Vec<u64> {
        self.active.keys().copied().collect()
    }
}

/// Terminal kind retained after payload disposal so late races stay classifiable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalKind {
    Cancelled,
    DeadlineExceeded,
    Response,
}

impl TerminalKind {
    fn of<T>(terminal: &InvocationTerminal<T>) -> Self {
        match terminal {
            InvocationTerminal::Cancelled => Self::Cancelled,
            InvocationTerminal::DeadlineExceeded => Self::DeadlineExceeded,
            InvocationTerminal::Response(_) => Self::Response,
        }
    }
}

/// Stable host-owned terminal-arbiter diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalError {
    ReusedCorrelationId,
    UnknownCorrelationId,
    SecondTerminal,
    RetiredIdCapacity,
}
