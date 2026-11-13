//! Native action execution, access checks and resource ownership for Lenso applications.
#![doc = include_str!("../README.md")]

mod access;
mod action;
mod console;
#[cfg(feature = "json")]
mod durable_event;
mod error;
mod events;
mod module;
mod resource;
#[cfg(feature = "json")]
mod task;

pub use access::{
    Principal, all_of, all_of_async, any_of, any_of_async, require_owner, require_principal,
    require_scope,
};
pub use action::{Action, CallRecord, Context, Executor, Observer};
#[cfg(feature = "json")]
pub use console::ConsoleExtension;
pub use console::{ConsoleManifest, ConsolePage};
#[cfg(feature = "json")]
pub use durable_event::{DurableEvent, EventDelivery};
pub use error::{ActionError, ErrorCode};
pub use events::Event;
pub use module::{Catalog, Module};
pub use resource::{Resource, ResourceError, connect, with_resource};
#[cfg(feature = "json")]
pub use task::{Task, TaskContext};
pub use tokio_util::sync::CancellationToken;
