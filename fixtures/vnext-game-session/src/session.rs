use std::{any::Any, cell::RefCell, collections::VecDeque, rc::Rc};

use futures::{channel::oneshot, future::LocalBoxFuture};
use lenso_capability_game_session::{PlayError, PlayResponse};
use lenso_kernel::{NativeStreamItem, NativeStreamSession, RuntimeFailure};

/// Selects the game behavior through App Composition rather than Kernel policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionMode {
    /// The default provider acknowledges each action.
    Echo,
    /// A replaceable provider prefixes each acknowledgment with its own identity.
    Replacement,
}

#[derive(Debug)]
struct SessionState {
    events: VecDeque<NativeStreamItem>,
    waiter: Option<oneshot::Sender<Result<NativeStreamItem, RuntimeFailure>>>,
    cancelled: bool,
    local_half_closed: bool,
    terminal: bool,
    max_buffered: usize,
}

impl SessionState {
    fn new(room: &str, subject: &str, mode: SessionMode, max_buffered: usize) -> Self {
        let prefix = match mode {
            SessionMode::Echo => "welcome",
            SessionMode::Replacement => "replacement-welcome",
        };
        let mut events = VecDeque::new();
        events.push_back(NativeStreamItem::Message(Box::new(PlayResponse {
            action: format!("{prefix}:{room}:{subject}"),
        })));
        Self {
            events,
            waiter: None,
            cancelled: false,
            local_half_closed: false,
            terminal: false,
            max_buffered: max_buffered.max(1),
        }
    }

    fn deliver(&mut self, event: NativeStreamItem) {
        if let Some(waiter) = self.waiter.take() {
            let _ = waiter.send(Ok(event));
        } else {
            self.events.push_back(event);
        }
    }
}

/// An in-process game provider session used behind the public stream seam.
#[derive(Debug)]
pub(crate) struct GameSession {
    state: Rc<RefCell<SessionState>>,
    mode: SessionMode,
}

impl GameSession {
    pub(crate) fn new(room: &str, subject: &str, mode: SessionMode, max_buffered: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(SessionState::new(
                room,
                subject,
                mode,
                max_buffered,
            ))),
            mode,
        }
    }
}

impl NativeStreamSession for GameSession {
    fn send(&self, message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let state = self.state.clone();
        let mode = self.mode;
        Box::pin(async move {
            let message = message.downcast::<PlayResponse>().map_err(|_| {
                RuntimeFailure::ProtocolViolation {
                    capability: lenso_capability_game_session::CAPABILITY_ID,
                }
            })?;
            let mut state = state.borrow_mut();
            if state.cancelled {
                return Err(RuntimeFailure::Cancelled { request_id: 0 });
            }
            if state.local_half_closed || state.terminal {
                return Err(RuntimeFailure::ProtocolViolation {
                    capability: lenso_capability_game_session::CAPABILITY_ID,
                });
            }
            if *message
                == (PlayResponse {
                    action: "crash".to_owned(),
                })
            {
                return Err(RuntimeFailure::ModuleFailure {
                    detail: "fixture game provider crashed while handling an action".to_owned(),
                });
            }
            if message.action == "quit" {
                if state.waiter.is_none() && state.events.len() >= state.max_buffered {
                    return Err(RuntimeFailure::ResourceExhausted {
                        capability: lenso_capability_game_session::CAPABILITY_ID,
                        operation: lenso_capability_game_session::PLAY_OPERATION.to_owned(),
                    });
                }
                state.terminal = true;
                state.deliver(NativeStreamItem::Terminal(Err(Box::new(
                    PlayError::RoomClosed,
                ))));
            } else {
                if state.waiter.is_none() && state.events.len() >= state.max_buffered {
                    return Err(RuntimeFailure::ResourceExhausted {
                        capability: lenso_capability_game_session::CAPABILITY_ID,
                        operation: lenso_capability_game_session::PLAY_OPERATION.to_owned(),
                    });
                }
                let prefix = match mode {
                    SessionMode::Echo => "ack",
                    SessionMode::Replacement => "replacement-ack",
                };
                state.deliver(NativeStreamItem::Message(Box::new(PlayResponse {
                    action: format!("{prefix}:{}", message.action),
                })));
            }
            Ok(())
        })
    }

    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        let state = self.state.clone();
        let result = {
            let mut state = state.borrow_mut();
            if let Some(event) = state.events.pop_front() {
                Ok(event)
            } else if state.cancelled {
                Err(RuntimeFailure::Cancelled { request_id: 0 })
            } else if state.waiter.is_some() {
                Err(RuntimeFailure::ProtocolViolation {
                    capability: lenso_capability_game_session::CAPABILITY_ID,
                })
            } else {
                let (sender, receiver) = oneshot::channel();
                state.waiter = Some(sender);
                return Box::pin(async move {
                    receiver.await.unwrap_or_else(|_| {
                        Err(RuntimeFailure::Internal {
                            detail: "game session receive waiter was dropped".to_owned(),
                        })
                    })
                });
            }
        };
        Box::pin(async move { result })
    }

    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let result = {
            let mut state = self.state.borrow_mut();
            if state.cancelled || state.terminal || state.local_half_closed {
                Err(RuntimeFailure::ProtocolViolation {
                    capability: lenso_capability_game_session::CAPABILITY_ID,
                })
            } else {
                state.local_half_closed = true;
                state.terminal = true;
                if state.waiter.is_none()
                    && state.events.len().saturating_add(2) > state.max_buffered
                {
                    state.local_half_closed = false;
                    state.terminal = false;
                    Err(RuntimeFailure::ResourceExhausted {
                        capability: lenso_capability_game_session::CAPABILITY_ID,
                        operation: lenso_capability_game_session::PLAY_OPERATION.to_owned(),
                    })
                } else {
                    state.deliver(NativeStreamItem::PeerHalfClosed);
                    state.deliver(NativeStreamItem::Terminal(Ok(())));
                    Ok(())
                }
            }
        };
        Box::pin(async move { result })
    }

    fn cancel(&self) {
        let mut state = self.state.borrow_mut();
        if state.cancelled || state.terminal {
            return;
        }
        state.cancelled = true;
        if let Some(waiter) = state.waiter.take() {
            let _ = waiter.send(Err(RuntimeFailure::Cancelled { request_id: 0 }));
        }
    }
}
